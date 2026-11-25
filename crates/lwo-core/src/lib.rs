//! Platform-independent core for the overlay: config, hotkey parsing, widget
//! traits and the clock widget. No OS code lives here.
//!
//! The product name is a placeholder. Everything that shows the name to the
//! user or the OS (config directory, window class, tray tooltip) must read it
//! from [`APP_NAME`] so a rename is a one-line change.

pub mod clock;
pub mod config;
pub mod form;
pub mod fps;
pub mod hotkey;
pub mod layout;
pub mod now_playing;
pub mod schedule;
pub mod stats;
pub mod theme;
pub mod units;
pub mod widget;

/// Placeholder product name. Change here only.
pub const APP_NAME: &str = "lightweight-overlay";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_name_is_not_empty() {
        assert!(!APP_NAME.is_empty());
    }
}
