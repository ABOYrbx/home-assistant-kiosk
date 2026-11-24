//! Generated Slint types and the bridge to `AppState`.
//!
//! `build.rs` compiles `ui/main.slint`; the generated `AppWindow` is available
//! from this module. The bridge and image helpers are implemented in later
//! waves.

slint::include_modules!();

pub mod bridge;
pub mod images;
