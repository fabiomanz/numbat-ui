# Numbat UI

> A beautiful, native, cross-platform wrapper for the [Numbat](https://numbat.dev/) scientific calculator.

Numbat UI brings the power of the Numbat scientific calculator to your desktop as a standalone application. Built with [Tauri](https://tauri.app/) and [Svelte 5](https://svelte.dev/), it offers a seamless, native experience on macOS, Windows, and Linux.

## Features

*   **Cross-Platform**: Runs natively on macOS, Windows, and Linux.
*   **Native Experience**: Utilizes system-native window frame and controls.
*   **Full Terminal Emulation**: Integrated `xterm.js` and `portable-pty` provide a robust CLI experience with full support for Numbat's features (syntax highlighting, history, interactive prompts).

## Installation

### Download Binaries

Check the [Releases](https://github.com/fabio/numbat_ui/releases) page for the latest installers:

*   **macOS**: `.dmg` or `.app`
*   **Windows**: `.exe` or `.msi`
*   **Linux**: `.deb`, `.rpm`, or `.AppImage`

### Homebrew (macOS)

You can install Numbat UI via Homebrew:

```bash
brew install fabio/tools/numbat-ui
```

### Build from Source

If you prefer to build it yourself, ensure you have the following installed:

*   [Rust](https://www.rust-lang.org/tools/install) (latest stable)
*   [Node.js](https://nodejs.org/) (LTS recommended)
*   [Numbat](https://github.com/sharkdp/numbat) (The `numbat` binary must be in your system PATH)

```bash
# Clone the repository
git clone https://github.com/fabio/numbat_ui.git
cd numbat_ui

# Install frontend dependencies
npm install

# Run in development mode (hot-reload)
npm run tauri dev

# Build for production
npm run tauri build
```

## 🛠️ Development

We welcome contributions! The project is structured as follows:

*   `src/`: Svelte 5 frontend code.
    *   `components/Terminal.svelte`: The core terminal component wrapping xterm.js.
*   `src-tauri/`: Rust backend code.
    *   `src/lib.rs`: Handles PTY spawning and communication.

### Key Commands

| Command | Description |
|Args|Description|
|---|---|
| `npm run tauri dev` | Starts the app in dev mode. |
| `npm run tauri build` | Builds a production bundle. |
| `npm run tauri icon` | Regenerates app icons from source. |

### Releasing

To release a new version, run the following command:

```bash
npm version <patch|minor|major>
```

This will automatically:
1.  Update the version in `package.json`, `src-tauri/tauri.conf.json`, and `src-tauri/Cargo.toml`.
2.  Create a git commit with the version bump.
3.  Create a git tag (e.g., `v1.2.3`).

Push the changes and the tag to GitHub to trigger the release workflow:

```bash
git push && git push --tags
```

## 🤝 Contributing

1.  Fork the repository.
2.  Create a new branch (`git checkout -b feature/amazing-feature`).
3.  Commit your changes (`git commit -m 'Add some amazing feature'`).
4.  Push to the branch (`git push origin feature/amazing-feature`).
5.  Open a Pull Request.

## 📄 License

This project is open source and available under the [MIT License](LICENSE).

## 🙏 Acknowledgments

*   **[Sharkdp](https://github.com/sharkdp)** for creating [Numbat](https://github.com/sharkdp/numbat), an incredible scientific calculator.
*   The **[Tauri](https://tauri.app/)** team for the amazing framework.
