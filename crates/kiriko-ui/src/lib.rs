//! Kiriko's UI shell (egui). Engine crates must never depend on this crate —
//! the dependency arrow points the other way (docs/05-ARCHITECTURE.md).

pub mod app_state;
pub mod shell;
pub mod theme;

pub use shell::Shell;
pub use theme::Theme;
