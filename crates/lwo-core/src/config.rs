//! TOML config at `%APPDATA%\<APP_NAME>\config.toml`.
//!
//! Missing file: defaults are written. Unreadable or invalid file: defaults are
//! used for this run and the file is left untouched so the user can fix it.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::hotkey::Hotkey;
use crate::layout::Placement;
use crate::theme::{Palette, Theme};
use crate::units::TemperatureUnit;
use crate::widget::{Color, Style};
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
    /// Edit mode: make the widgets draggable. Pressing it again saves the layout.
    pub edit: String,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            toggle: "Ctrl+Alt+O".into(),
            edit: "Ctrl+Alt+E".into(),
        }
    }
}

/// How often a widget asks for fresh data: 1 to 60 seconds.
pub const DEFAULT_REFRESH_SECONDS: u32 = 1;
pub const MAX_REFRESH_SECONDS: u32 = 60;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppearanceConfig {
    pub theme: Theme,
    /// Optional hex overrides such as "#FFFFFF" or "#00000080". Empty means
    /// "use the theme's colour".
    pub text_color: String,
    pub background_color: String,
    pub accent_color: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UnitsConfig {
    pub temperature: TemperatureUnit,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClockConfig {
    pub enabled: bool,
    pub refresh_seconds: u32,
    pub show_seconds: bool,
    pub use_24h: bool,
}

impl Default for ClockConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            refresh_seconds: DEFAULT_REFRESH_SECONDS,
            show_seconds: true,
            use_24h: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NowPlayingConfig {
    pub enabled: bool,
    /// How often the progress bar ticks while playing. Track changes always
    /// show immediately.
    pub refresh_seconds: u32,
    /// Append the album to the artist line.
    pub show_album: bool,
    /// Hide the panel completely while no media session exists.
    pub hide_when_idle: bool,
}

impl Default for NowPlayingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            refresh_seconds: DEFAULT_REFRESH_SECONDS,
            show_album: false,
            hide_when_idle: false,
        }
    }
}

/// The FPS widget. Off by default: it needs a Windows privilege, and how
/// anti-cheat systems treat reading frame events is not known.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FpsConfig {
    pub enabled: bool,
    pub refresh_seconds: u32,
}

impl Default for FpsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            refresh_seconds: DEFAULT_REFRESH_SECONDS,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CpuConfig {
    pub enabled: bool,
    pub refresh_seconds: u32,
    /// Show CPU package power where Windows reports it (hidden elsewhere).
    pub show_power: bool,
}

impl Default for CpuConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            refresh_seconds: DEFAULT_REFRESH_SECONDS,
            show_power: true,
        }
    }
}

/// Settings shared by the stat widgets (memory, GPU).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ToggleConfig {
    pub enabled: bool,
    pub refresh_seconds: u32,
}

impl Default for ToggleConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            refresh_seconds: DEFAULT_REFRESH_SECONDS,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WidgetsConfig {
    pub clock: ClockConfig,
    pub now_playing: NowPlayingConfig,
    pub cpu: CpuConfig,
    pub ram: ToggleConfig,
    /// NVIDIA GPU. Quietly hidden when no NVIDIA driver (NVML) is installed.
    pub gpu: ToggleConfig,
    pub fps: FpsConfig,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub overlay: OverlayConfig,
    pub appearance: AppearanceConfig,
    pub units: UnitsConfig,
    pub hotkeys: HotkeyConfig,
    pub widgets: WidgetsConfig,
    /// Where each widget was dragged to, keyed by widget id. A widget with no
    /// entry sits in the default stack at `overlay.anchor` and offset.
    pub layout: BTreeMap<String, Placement>,
}

/// Widget ids in their default stacking order.
pub const WIDGET_IDS: [&str; 6] = ["clock", "now_playing", "cpu", "ram", "gpu", "fps"];

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
        if o.font.trim().is_empty() {
            o.font = OverlayConfig::default().font;
        }
        let w = &mut self.widgets;
        for secs in [
            &mut w.clock.refresh_seconds,
            &mut w.now_playing.refresh_seconds,
            &mut w.cpu.refresh_seconds,
            &mut w.ram.refresh_seconds,
            &mut w.gpu.refresh_seconds,
            &mut w.fps.refresh_seconds,
        ] {
            *secs = (*secs).clamp(1, MAX_REFRESH_SECONDS);
        }
    }

    /// Whether the widget with this id is switched on.
    pub fn enabled(&self, id: &str) -> bool {
        let w = &self.widgets;
        match id {
            "clock" => w.clock.enabled,
            "now_playing" => w.now_playing.enabled,
            "cpu" => w.cpu.enabled,
            "ram" => w.ram.enabled,
            "gpu" => w.gpu.enabled,
            "fps" => w.fps.enabled,
            _ => false,
        }
    }

    /// The slowest this widget may be updated, from its `refresh_seconds`.
    pub fn refresh_for(&self, id: &str) -> Duration {
        let w = &self.widgets;
        let secs = match id {
            "clock" => w.clock.refresh_seconds,
            "now_playing" => w.now_playing.refresh_seconds,
            "cpu" => w.cpu.refresh_seconds,
            "ram" => w.ram.refresh_seconds,
            "gpu" => w.gpu.refresh_seconds,
            "fps" => w.fps.refresh_seconds,
            _ => DEFAULT_REFRESH_SECONDS,
        };
        Duration::from_secs(u64::from(secs.clamp(1, MAX_REFRESH_SECONDS)))
    }

    /// The theme's colours with any valid hex overrides applied. An invalid
    /// override is ignored and reported in the second value.
    pub fn palette(&self) -> (Palette, Vec<String>) {
        let a = &self.appearance;
        let mut p = a.theme.palette();
        let mut problems = Vec::new();
        for (name, text, slot) in [
            ("text", &a.text_color, &mut p.text),
            ("background", &a.background_color, &mut p.background),
            ("accent", &a.accent_color, &mut p.accent),
        ] {
            let text = text.trim();
            if text.is_empty() {
                continue;
            }
            match Color::from_hex(text) {
                Some(c) => *slot = c,
                None => problems.push(format!(
                    "The {name} colour {text:?} is not a hex colour like #RRGGBB or #RRGGBBAA; using the theme's colour."
                )),
            }
        }
        (p, problems)
    }

    /// The drawing style for the current settings at the given display scaling.
    pub fn style(&self, dpi_scale: f32) -> Style {
        let (p, _) = self.palette();
        Style {
            font: self.overlay.font.clone(),
            scale: self.overlay.scale * dpi_scale,
            text: p.text,
            background: p.background,
            accent: p.accent,
            temperature_unit: self.units.temperature,
        }
    }

    /// Everything wrong with these settings that the user should fix: bad
    /// colours, unparsable or clashing hotkeys. Empty means all good.
    pub fn problems(&self) -> Vec<String> {
        let mut out = self.palette().1;
        let toggle = Hotkey::parse(&self.hotkeys.toggle);
        let edit = Hotkey::parse(&self.hotkeys.edit);
        if let Err(e) = &toggle {
            out.push(format!("Show/hide hotkey: {e}."));
        }
        if let Err(e) = &edit {
            out.push(format!("Edit-mode hotkey: {e}."));
        }
        if let (Ok(a), Ok(b)) = (toggle, edit) {
            if a == b {
                out.push("The show/hide and edit-mode hotkeys must be different.".into());
            }
        }
        out
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
        c.widgets.gpu.enabled = false;
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

    #[test]
    fn a_milestone_3_config_still_loads_and_gets_the_new_defaults() {
        let dir = temp_dir("old");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        fs::write(
            &path,
            "[overlay]\nanchor = \"top-right\"\n[hotkeys]\ntoggle = \"Ctrl+Alt+P\"\n[widgets.clock]\nenabled = false\n[widgets.cpu]\nenabled = true\n",
        )
        .unwrap();
        let loaded = Config::load_or_create(&path);
        assert!(loaded.warning.is_none());
        let c = loaded.config;
        assert_eq!(c.overlay.anchor, Anchor::TopRight);
        assert_eq!(c.hotkeys.toggle, "Ctrl+Alt+P");
        assert_eq!(
            c.hotkeys.edit, "Ctrl+Alt+E",
            "new field falls back to its default"
        );
        assert!(!c.widgets.clock.enabled);
        assert_eq!(c.widgets.cpu.refresh_seconds, 1);
        assert_eq!(c.appearance.theme, Theme::Dark);
        assert_eq!(c.units.temperature, TemperatureUnit::Celsius);
        assert!(c.layout.is_empty());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn every_new_setting_round_trips_through_the_file() {
        let dir = temp_dir("roundtrip2");
        let path = dir.join("config.toml");
        let mut c = Config::default();
        c.appearance.theme = Theme::Midnight;
        c.appearance.accent_color = "#FF00AA".into();
        c.units.temperature = TemperatureUnit::Fahrenheit;
        c.hotkeys.edit = "Ctrl+Shift+F8".into();
        c.widgets.cpu.refresh_seconds = 5;
        c.widgets.now_playing.hide_when_idle = true;
        c.layout.insert(
            "clock".into(),
            Placement {
                monitor: 1,
                anchor: Anchor::BottomRight,
                x: 33,
                y: 44,
            },
        );
        c.save(&path).unwrap();
        let loaded = Config::load_or_create(&path);
        assert!(loaded.warning.is_none(), "{:?}", loaded.warning);
        assert_eq!(loaded.config, c);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn refresh_seconds_is_clamped_to_1_through_60() {
        let mut c = Config::default();
        c.widgets.cpu.refresh_seconds = 0;
        c.widgets.ram.refresh_seconds = 9999;
        c.sanitize();
        assert_eq!(c.widgets.cpu.refresh_seconds, 1);
        assert_eq!(c.widgets.ram.refresh_seconds, 60);
        assert_eq!(c.refresh_for("cpu"), Duration::from_secs(1));
        assert_eq!(c.refresh_for("ram"), Duration::from_secs(60));
    }

    #[test]
    fn an_empty_font_is_replaced_by_the_default() {
        let mut c = Config::default();
        c.overlay.font = "   ".into();
        c.sanitize();
        assert_eq!(c.overlay.font, "Segoe UI");
    }

    #[test]
    fn theme_colours_apply_and_valid_overrides_win() {
        let mut c = Config::default();
        c.appearance.theme = Theme::Light;
        let (p, problems) = c.palette();
        assert!(problems.is_empty());
        assert_eq!(p, Theme::Light.palette());

        c.appearance.text_color = "#FF0000".into();
        c.appearance.background_color = "00000000".into();
        let (p, problems) = c.palette();
        assert!(problems.is_empty());
        assert_eq!(p.text.to_hex(), "#FF0000");
        assert_eq!(p.background.a, 0.0);
        assert_eq!(
            p.accent,
            Theme::Light.palette().accent,
            "accent not overridden"
        );
    }

    #[test]
    fn an_invalid_colour_is_reported_and_falls_back_to_the_theme() {
        let mut c = Config::default();
        c.appearance.accent_color = "not a colour".into();
        let (p, problems) = c.palette();
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("accent"));
        assert_eq!(p.accent, Theme::Dark.palette().accent);
        assert_eq!(c.problems().len(), 1);
    }

    #[test]
    fn style_combines_scale_font_colours_and_unit() {
        let mut c = Config::default();
        c.overlay.scale = 1.5;
        c.overlay.font = "Consolas".into();
        c.units.temperature = TemperatureUnit::Fahrenheit;
        let s = c.style(2.0);
        assert_eq!(s.scale, 3.0);
        assert_eq!(s.font, "Consolas");
        assert_eq!(s.temperature_unit, TemperatureUnit::Fahrenheit);
    }

    #[test]
    fn problems_catches_bad_and_clashing_hotkeys() {
        let mut c = Config::default();
        assert!(c.problems().is_empty(), "defaults are valid");
        c.hotkeys.toggle = "O".into(); // no modifier
        assert_eq!(c.problems().len(), 1);
        c.hotkeys.toggle = "Ctrl+Alt+E".into(); // same as the edit hotkey
        let p = c.problems();
        assert_eq!(p.len(), 1);
        assert!(p[0].contains("different"));
    }

    #[test]
    fn enabled_and_refresh_lookups_cover_every_widget_id() {
        let mut c = Config::default();
        for id in WIDGET_IDS.into_iter().filter(|&id| id != "fps") {
            assert!(c.enabled(id), "{id} is on by default");
        }
        assert!(!c.enabled("fps"), "FPS is opt-in");
        assert!(!c.enabled("nonsense"));
        c.widgets.gpu.enabled = false;
        assert!(!c.enabled("gpu"));
    }
}
