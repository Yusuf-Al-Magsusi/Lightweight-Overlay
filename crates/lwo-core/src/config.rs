//! TOML config at `%APPDATA%\<APP_NAME>\config.toml`.
//!
//! Missing file: defaults are written. Unreadable or invalid file: defaults are
//! used for this run and the file is left untouched so the user can fix it.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::APP_NAME;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Anchor {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OverlayConfig {
    /// Show the overlay at startup.
    pub start_visible: bool,
    pub anchor: Anchor,
    /// Distance in pixels from the anchored corner of the primary monitor, at 100% DPI.
    pub offset_x: i32,
    pub offset_y: i32,
    /// 0.5..=4.0
    pub scale: f32,
    /// 0.1..=1.0, applied to the whole overlay.
    pub opacity: f32,
    pub font: String,
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self {
            start_visible: true,
            anchor: Anchor::TopLeft,
            offset_x: 20,
            offset_y: 20,
            scale: 1.0,
            opacity: 1.0,
            font: "Segoe UI".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HotkeyConfig {
    /// Show or hide the overlay. Format: "Ctrl+Alt+O".
    pub toggle: String,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            toggle: "Ctrl+Alt+O".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClockConfig {
    pub enabled: bool,
    pub show_seconds: bool,
    pub use_24h: bool,
}

impl Default for ClockConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            show_seconds: true,
            use_24h: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NowPlayingConfig {
    pub enabled: bool,
    /// Append the album to the artist line.
    pub show_album: bool,
}

impl Default for NowPlayingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            show_album: false,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WidgetsConfig {
    pub clock: ClockConfig,
    pub now_playing: NowPlayingConfig,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub overlay: OverlayConfig,
    pub hotkeys: HotkeyConfig,
    pub widgets: WidgetsConfig,
}

/// Result of loading: the config to use, plus a message for the user if the
/// file could not be used.
pub struct Loaded {
    pub config: Config,
    pub warning: Option<String>,
}

/// `%APPDATA%\<APP_NAME>\config.toml`, or `None` if `APPDATA` is not set.
pub fn default_path() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(appdata).join(APP_NAME).join("config.toml"))
}

impl Config {
    /// Clamp out-of-range values instead of rejecting the whole file.
    pub fn sanitize(&mut self) {
        let o = &mut self.overlay;
        o.scale = if o.scale.is_finite() {
            o.scale.clamp(0.5, 4.0)
        } else {
            1.0
        };
        o.opacity = if o.opacity.is_finite() {
            o.opacity.clamp(0.1, 1.0)
        } else {
            1.0
        };
    }

    pub fn load_or_create(path: &Path) -> Loaded {
        match fs::read_to_string(path) {
            Ok(text) => match toml::from_str::<Config>(&text) {
                Ok(mut config) => {
                    config.sanitize();
                    Loaded {
                        config,
                        warning: None,
                    }
                }
                Err(e) => Loaded {
                    config: Config::default(),
                    warning: Some(format!(
                        "Could not parse {}: {e}. Using defaults; the file was not changed.",
                        path.display()
                    )),
                },
            },
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let config = Config::default();
                let warning = config
                    .save(path)
                    .err()
                    .map(|e| format!("Could not create {}: {e}", path.display()));
                Loaded { config, warning }
            }
            Err(e) => Loaded {
                config: Config::default(),
                warning: Some(format!(
                    "Could not read {}: {e}. Using defaults.",
                    path.display()
                )),
            },
        }
    }

    /// Write atomically: temp file in the same folder, then rename over the target.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let text = toml::to_string_pretty(self).map_err(io::Error::other)?;
        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, text)?;
        fs::rename(&tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lwo-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn missing_file_creates_defaults() {
        let dir = temp_dir("missing");
        let path = dir.join("config.toml");
        let loaded = Config::load_or_create(&path);
        assert!(loaded.warning.is_none());
        assert_eq!(loaded.config, Config::default());
        assert!(path.exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = temp_dir("roundtrip");
        let path = dir.join("config.toml");
        let mut c = Config::default();
        c.overlay.anchor = Anchor::BottomRight;
        c.overlay.scale = 1.5;
        c.hotkeys.toggle = "Ctrl+Shift+F9".into();
        c.widgets.clock.use_24h = false;
        c.widgets.now_playing.show_album = true;
        c.save(&path).unwrap();
        let loaded = Config::load_or_create(&path);
        assert!(loaded.warning.is_none());
        assert_eq!(loaded.config, c);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn partial_file_fills_in_defaults() {
        let dir = temp_dir("partial");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        fs::write(&path, "[widgets.clock]\nshow_seconds = false\n").unwrap();
        let loaded = Config::load_or_create(&path);
        assert!(loaded.warning.is_none());
        assert!(!loaded.config.widgets.clock.show_seconds);
        assert_eq!(loaded.config.overlay, OverlayConfig::default());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn invalid_file_uses_defaults_and_is_not_overwritten() {
        let dir = temp_dir("invalid");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        fs::write(&path, "this is = not [valid toml").unwrap();
        let loaded = Config::load_or_create(&path);
        assert!(loaded.warning.is_some());
        assert_eq!(loaded.config, Config::default());
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "this is = not [valid toml"
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn out_of_range_values_are_clamped() {
        let dir = temp_dir("clamp");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        fs::write(&path, "[overlay]\nscale = 99.0\nopacity = 0.0\n").unwrap();
        let loaded = Config::load_or_create(&path);
        assert_eq!(loaded.config.overlay.scale, 4.0);
        assert_eq!(loaded.config.overlay.opacity, 0.1);
        let _ = fs::remove_dir_all(dir);
    }
}
