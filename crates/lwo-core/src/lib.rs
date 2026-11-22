//! Core types for the overlay.
//!
//! The product name is a placeholder. Everything that shows the name to the
//! user or the OS (config directory, window class, tray tooltip) must read it
//! from [`APP_NAME`] so a rename is a one-line change.

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
