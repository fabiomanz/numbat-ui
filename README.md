# Numbat UI

> A beautiful, native, cross-platform UI for the [Numbat](https://numbat.dev/) scientific calculator.

Numbat UI brings the power of the Numbat scientific calculator to your desktop as a standalone application. Built purely in [Rust](https://www.rust-lang.org/) using [egui](https://github.com/emilk/egui), it offers a fast, native experience with zero web-views on macOS, Windows, and Linux.

## Features

*   **Quick panel** — press a global hotkey (default: `Option+Space` on macOS, `Ctrl+Alt+Space` elsewhere) anywhere in your OS to summon a Spotlight-style calculator. Type, read the result, copy it, and dismiss — or press `Cmd/Ctrl+Enter` to continue the calculation in the full window.
*   **Live results** — the answer appears as you type, before you press Enter.
*   **Copy anywhere** — click any result to copy it; `Cmd/Ctrl+Shift+C` copies the latest one.
*   **Full Numbat power** — physical units, conversions, variables, functions, currencies, and readable compiler-style error messages with source spans.
*   **Tab completion** — complete unit, function and variable names with `Tab`.
*   **Persistent sessions** — your history (and all definitions in it) survive restarts; the quick panel and main window share one session.
*   **Background mode** — closing the window removes the app from the Dock but keeps the hotkey alive; optionally launch (hidden) at login so the quick panel is always one keystroke away.
*   **Modern UI** — dark and light themes (follows the system by default), card-based history with syntax highlighting.
*   **Cross-platform & native** — one Rust binary for macOS, Windows and Linux. No Electron, no web-view.

## Keyboard reference

| Keys | Action |
|---|---|
| `Option+Space` / `Ctrl+Alt+Space` | Toggle the quick panel (global, configurable) |
| `Enter` | Evaluate |
| `Cmd/Ctrl+Enter` | (quick panel) Continue in the full window |
| `Cmd/Ctrl+C` | (quick panel) Copy the current result |
| `Cmd/Ctrl+Shift+C` | Copy the latest result |
| `Tab` | Complete names; press again to cycle candidates |
| `↑` / `↓` | Browse input history |
| `Cmd/Ctrl+L` | Clear the history view |
| `Esc` | Dismiss the quick panel / completion popup |

The prompt also understands the REPL commands `help`, `list`, `info <name>`, `clear` and `reset`.

> Closing the main window keeps Numbat running in the background (on macOS it also leaves the Dock) so the quick panel stays available. Quit for real via the menu or `Cmd/Ctrl+Q`. Enable *Launch at login* in the settings to have the hotkey ready right after boot — the app then starts hidden (`--hidden` flag).

## Installation

### Download Binaries

Check the [Releases](https://github.com/fabiomanz/numbat-ui/releases) page for the latest executables:

*   **macOS**: universal app bundle (Intel + Apple Silicon)
*   **Windows**: `.exe` built for `x86_64`
*   **Linux**: executable built for `x86_64` (the global hotkey requires X11/XWayland)

### Homebrew (macOS)

```bash
brew install fabiomanz/tools/numbat-ui
```

### Build from Source

Requires [Rust](https://www.rust-lang.org/tools/install) (latest stable).

```bash
git clone https://github.com/fabiomanz/numbat-ui.git
cd numbat-ui

# Run in development mode
cargo run

# Build for production
cargo build --release
```

## Configuration

Settings live in the app (gear icon, or `Cmd+,` on macOS) and are stored as TOML in your config directory (e.g. `~/Library/Application Support/numbat-ui/config.toml` on macOS):

```toml
[formatting]
digit-separator = "_"          # "_", ",", " ", "'" or "" to disable
digit-grouping-threshold = 6   # group digits starting at this many
significant-digits = 6

[ui]
theme = "system"               # "system", "dark" or "light"
quick-panel-hotkey = "Alt+Space"
font-size = 14.0
launch-at-login = false        # start hidden at login (managed from the settings UI)
```

On first launch, formatting options are migrated from an existing numbat CLI config if present.

## 🛠️ Development

*   `src/` — application code: `engine.rs` (numbat wrapper), `session.rs` (shared calculator session), `ui/` (main window, quick panel, settings), `theme.rs`, `hotkey.rs`, `platform.rs`.
*   `tests/` — integration tests.

| Command | Description |
|---|---|
| `cargo run` | Starts the app in dev mode. |
| `cargo build --release` | Builds an optimized binary in `target/release`. |
| `cargo test` | Runs the test suite. |

Debug builds include a screenshot harness for UI verification: `NUMBAT_UI_SHOT=/tmp/shots cargo run` captures the main window, quick panel and settings as PNGs, then exits.

### Releasing

Update `version` in `Cargo.toml` (or `cargo set-version <patch|minor|major>` from cargo-edit), commit, tag (e.g. `v3.0.0`), then:

```bash
git push && git push --tags
```

The release workflow builds and uploads binaries for all platforms and bumps the Homebrew formula.

## 🤝 Contributing

1.  Fork the repository.
2.  Create a new branch (`git checkout -b feature/amazing-feature`).
3.  Commit your changes (`git commit -m 'Add some amazing feature'`).
4.  Push to the branch (`git push origin feature/amazing-feature`).
5.  Open a Pull Request.

## 📄 License

This project is open source and available under the [MIT License](LICENSE).

*This application includes the **JetBrains Mono** font, licensed under the [SIL Open Font License 1.1](https://scripts.sil.org/OFL).*

## 🙏 Acknowledgments

*   **[Sharkdp](https://github.com/sharkdp)** for creating [Numbat](https://github.com/sharkdp/numbat), an incredible scientific calculator.
*   The **[egui](https://github.com/emilk/egui)** UI library.
