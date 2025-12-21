use portable_pty::{CommandBuilder, MasterPty, NativePtySystem, PtySize, PtySystem};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use tauri::{Emitter, State};

struct PtyState {
    master: Arc<Mutex<Option<Box<dyn MasterPty + Send>>>>,
    writer: Arc<Mutex<Option<Box<dyn Write + Send>>>>,
}

#[tauri::command]
fn async_pty_spawn(state: State<'_, PtyState>, window: tauri::Window) {
    let pty_system = NativePtySystem::default();

    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("Failed to create PTY");

    // Resolve the path to the bundled sidecar
    let mut numbat_path = std::env::current_exe().expect("failed to get current exe");
    numbat_path.pop();
    if cfg!(target_os = "windows") {
        numbat_path.push("numbat.exe");
    } else {
        numbat_path.push("numbat");
    }

    let cmd = CommandBuilder::new(numbat_path);
    let _child = pair
        .slave
        .spawn_command(cmd)
        .expect("Failed to spawn numbat");

    let mut reader = pair
        .master
        .try_clone_reader()
        .expect("Failed to clone reader");
    let writer = pair.master.take_writer().expect("Failed to take writer");
    let master = pair.master;

    // Store master and writer in state
    *state.master.lock().unwrap() = Some(master);
    *state.writer.lock().unwrap() = Some(writer);

    // Spawn thread to read from PTY
    thread::spawn(move || {
        let mut buffer = [0u8; 1024];
        loop {
            match reader.read(&mut buffer) {
                Ok(n) if n > 0 => {
                    let data = String::from_utf8_lossy(&buffer[..n]).to_string();
                    window.emit("term-data", data).unwrap_or(());
                }
                _ => {
                    // EOF or Error -> Process exited
                    window.close().unwrap_or(());
                    break;
                }
            }
        }
    });
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(PtyState {
            master: Arc::new(Mutex::new(None)),
            writer: Arc::new(Mutex::new(None)),
        })
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            async_pty_spawn,
            write_to_pty,
            resize_pty
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
