# Numbat UI

> A beautiful, native, cross-platform UI for the [Numbat](https://numbat.dev/) scientific calculator.

Numbat UI brings the power of the Numbat scientific calculator to your desktop as a standalone application. Built purely in [Rust](https://www.rust-lang.org/) using [egui](https://github.com/emilk/egui), it offers a fast, native experience with zero web-views on macOS, Windows, and Linux.

## Features

*   **Cross-Platform**: Runs natively on macOS, Windows, and Linux.
*   **Native Experience**: Fully written in Rust for exceptional performance and low resource usage.
*   **Integrated Calculator**: Seamless experience with Numbat's powerful features (syntax highlighting, history, interactive prompts).

## Installation

### Download Binaries

Check the [Releases](https://github.com/fabiomanz/numbat-ui/releases) page for the latest executables:

*   **macOS**: Intel (`x86_64`) or Apple Silicon (`aarch64`)
*   **Windows**: `.exe` built for `x86_64`
*   **Linux**: Executable built for `x86_64`

### Homebrew (macOS)

You can install Numbat UI via Homebrew:

```bash
brew install fabiomanz/tools/numbat-ui
```

### Build from Source

If you prefer to build it yourself, ensure you have the following installed:

*   [Rust](https://www.rust-lang.org/tools/install) (latest stable)


```bash
# Clone the repository
git clone https://github.com/fabiomanz/numbat-ui.git
cd numbat-ui

# Run in development mode
cargo run

# Build for production
cargo build --release
```

## 🛠️ Development

We welcome contributions! The project is structured entirely around Rust and egui.

*   `src/`: Application source code, containing egui components and application state handling.
*   `tests/`: Integration tests.

### Key Commands

| Command | Description |
|---|---|
| `cargo run` | Starts the app in dev mode. |
| `cargo build --release` | Builds an optimized binary in `target/release`. |
| `cargo test` | Runs the test suite. |

### Releasing

To release a new version, run the following command to update `Cargo.toml`:

```bash
cargo install cargo-edit # If you do not have cargo set-version
cargo set-version <patch|minor|major>
```

Alternatively, just update `version` in `Cargo.toml`.

Then:
1.  Create a git commit with the version bump.
2.  Create a git tag (e.g., `v1.2.3`).

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

*This application includes the **JetBrains Mono** font, licensed under the [SIL Open Font License 1.1](https://scripts.sil.org/OFL).*

## 🙏 Acknowledgments

*   **[Sharkdp](https://github.com/sharkdp)** for creating [Numbat](https://github.com/sharkdp/numbat), an incredible scientific calculator.
*   The **[egui](https://github.com/emilk/egui)** UI library.
