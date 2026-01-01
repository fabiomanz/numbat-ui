use portable_pty::{CommandBuilder, MasterPty, NativePtySystem, PtySize, PtySystem};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use tauri::{AppHandle, Emitter, Manager, State};

struct PtyState {
    master: Arc<Mutex<Option<Box<dyn MasterPty + Send>>>>,
    writer: Arc<Mutex<Option<Box<dyn Write + Send>>>>,
    // Buffer for data received before the frontend is ready
    buffer: Arc<Mutex<Vec<u8>>>,
    // Whether to stream directly to the frontend (true after init_pty is called)
    streaming: Arc<Mutex<bool>>,
}

#[tauri::command]
fn init_pty(state: State<'_, PtyState>) -> String {
    let mut buffer = state.buffer.lock().unwrap();
    let mut streaming = state.streaming.lock().unwrap();

    let output = String::from_utf8_lossy(&buffer).to_string();
    buffer.clear();
    *streaming = true;

    output
}

#[tauri::command]
fn write_to_pty(state: State<'_, PtyState>, data: String) {
    if let Some(writer) = state.writer.lock().unwrap().as_mut() {
        write!(writer, "{}", data).unwrap_or(());
    }
}

#[tauri::command]
fn resize_pty(state: State<'_, PtyState>, rows: u16, cols: u16) {
    if let Some(master) = state.master.lock().unwrap().as_mut() {
        master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap_or(());
    }
}

// Trait to abstract application lifecycle events for testing
trait PtyLifecycleHandler: Send + Sync + 'static {
    fn on_data(&self, data: String);
    fn on_exit(&self);
}

struct TauriPtyHandler {
    app_handle: AppHandle,
}

impl PtyLifecycleHandler for TauriPtyHandler {
    fn on_data(&self, data: String) {
        self.app_handle.emit("term-data", data).unwrap_or(());
    }

    fn on_exit(&self) {
        self.app_handle.exit(0);
    }
}

fn monitor_pty<R: Read + Send + 'static, H: PtyLifecycleHandler>(
    mut reader: R,
    handler: H,
    streaming: Arc<Mutex<bool>>,
    buffer: Arc<Mutex<Vec<u8>>>,
) {
    thread::spawn(move || {
        let mut buf = [0u8; 1024];
        loop {
            match reader.read(&mut buf) {
                Ok(n) if n > 0 => {
                    let is_streaming = *streaming.lock().unwrap();
                    if is_streaming {
                        let data = String::from_utf8_lossy(&buf[..n]).to_string();
                        handler.on_data(data);
                    } else {
                        buffer.lock().unwrap().extend_from_slice(&buf[..n]);
                    }
                }
                _ => {
                    // EOF or Error -> Process exited
                    handler.on_exit();
                    break;
                }
            }
        }
    });
}

pub fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let pty_system = NativePtySystem::default();

    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("Failed to create PTY");

    // Resolve the path to the current executable (self-spawn)
    let numbat_path = std::env::current_exe().expect("failed to get current exe");

    let mut cmd = CommandBuilder::new(numbat_path);
    cmd.args(&["--repl"]);

    let _child = pair
        .slave
        .spawn_command(cmd)
        .expect("Failed to spawn numbat");

    let reader = pair
        .master
        .try_clone_reader()
        .expect("Failed to clone reader");
    let writer = pair.master.take_writer().expect("Failed to take writer");
    let master = pair.master;

    let buffer = Arc::new(Mutex::new(Vec::new()));
    let streaming = Arc::new(Mutex::new(false));

    let state = PtyState {
        master: Arc::new(Mutex::new(Some(master))),
        writer: Arc::new(Mutex::new(Some(writer))),
        buffer: buffer.clone(),
        streaming: streaming.clone(),
    };

    app.manage(state);

    let app_handle = app.handle().clone();
    let buffer_clone = buffer.clone();
    let streaming_clone = streaming.clone();

    let handler = TauriPtyHandler { app_handle };
    monitor_pty(reader, handler, streaming_clone, buffer_clone);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    struct MockHandler {
        exit_called: Arc<Mutex<bool>>,
        data_received: Arc<Mutex<Vec<String>>>,
    }

    impl PtyLifecycleHandler for MockHandler {
        fn on_data(&self, data: String) {
            self.data_received.lock().unwrap().push(data);
        }

        fn on_exit(&self) {
            *self.exit_called.lock().unwrap() = true;
        }
    }

    #[test]
    fn test_monitor_pty_exits_on_eof() {
        let exit_called = Arc::new(Mutex::new(false));
        let data_received = Arc::new(Mutex::new(Vec::new()));

        let handler = MockHandler {
            exit_called: exit_called.clone(),
            data_received: data_received.clone(),
        };

        let streaming = Arc::new(Mutex::new(true));
        let buffer = Arc::new(Mutex::new(Vec::new()));

        // Simulating a reader that returns EOF immediately
        let reader = Cursor::new(vec![]);

        monitor_pty(reader, handler, streaming, buffer);

        // Give the thread a moment to run
        thread::sleep(Duration::from_millis(100));

        assert!(
            *exit_called.lock().unwrap(),
            "on_exit should have been called"
        );
    }

    #[test]
    fn test_monitor_pty_buffers_data_when_not_streaming() {
        let exit_called = Arc::new(Mutex::new(false));
        let data_received = Arc::new(Mutex::new(Vec::new()));

        let handler = MockHandler {
            exit_called: exit_called.clone(),
            data_received: data_received.clone(),
        };

        let streaming = Arc::new(Mutex::new(false));
        let buffer = Arc::new(Mutex::new(Vec::new()));

        // Simulate some data then EOF
        let reader = Cursor::new(vec![65, 66, 67]); // "ABC"

        monitor_pty(reader, handler, streaming, buffer.clone());

        thread::sleep(Duration::from_millis(100));

        // Should not have received data in handler
        assert!(data_received.lock().unwrap().is_empty());

        // Should have buffered data
        let buffered = buffer.lock().unwrap();
        assert_eq!(*buffered, vec![65, 66, 67]);

        // Should have exited
        assert!(*exit_called.lock().unwrap());
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        //.manage(PtyState { ... }) // Removed in favor of setup
        .setup(setup)
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![init_pty, write_to_pty, resize_pty])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
