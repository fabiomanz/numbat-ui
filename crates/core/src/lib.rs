//! The platform-independent half of Numbat UI: the numbat interpreter
//! wrapper, the calculator session on top of it, and the formatting options
//! both are configured with.
//!
//! Everything here builds for `wasm32-unknown-unknown` as well as for the
//! desktop targets — no filesystem, no windowing, no UI toolkit. Front ends
//! (`numbat-ui` for the desktop, `numbat-ui-web` for the PWA) supply the
//! platform pieces through [`session::HistoryStore`].

pub mod engine;
pub mod format;
pub mod session;

pub use engine::{Engine, EvalError, EvalOutput};
pub use format::FormattingConfig;
pub use session::{HistoryEntry, HistoryStore, NoHistory, Preview, Session};
