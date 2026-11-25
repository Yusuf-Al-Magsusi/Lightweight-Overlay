//! The settings form as plain data. The platform's dialog fills one in from the
//! controls, and this module turns it into config changes, validating the text
//! the user typed. No UI code here, so it is fully unit-tested.

use crate::config::{Config, MAX_REFRESH_SECONDS};
use crate::theme::Theme;
use crate::units::TemperatureUnit;

/// Everything the dialog edits. Numbers and colours stay as text until `apply_to`.
#[derive(Clone, Debug, PartialEq)]
pub struct SettingsForm {
    pub clock_enabled: bool,
    pub clock_refresh: String,
    pub clock_seconds: bool,
    pub clock_24h: bool,

    pub now_playing_enabled: bool,
    pub now_playing_refresh: String,
    pub now_playing_album: bool,
    pub now_playing_hide_idle: bool,

    pub cpu_enabled: bool,
    pub cpu_refresh: String,
    pub ram_enabled: bool,
    pub ram_refresh: String,
    pub gpu_enabled: bool,
    pub gpu_refresh: String,
    pub fps_enabled: bool,
    pub fps_refresh: String,

    pub theme: Theme,
    pub temperature: TemperatureUnit,
    /// 50 to 400.
    pub scale_percent: String,
    /// 10 to 100.
    pub opacity_percent: String,
    pub font: String,
    pub text_color: String,
    pub background_color: String,
    pub accent_color: String,

    pub hotkey_toggle: String,
    pub hotkey_edit: String,
}

fn whole_number(
    label: &str,
    text: &str,
    min: u32,
    max: u32,
    errors: &mut Vec<String>,
) -> Option<u32> {
    match text.trim().parse::<u32>() {
        Ok(n) if (min..=max).contains(&n) => Some(n),
        _ => {
            errors.push(format!(
                "{label} must be a whole number from {min} to {max}."
            ));
            None
        }
    }
}

impl SettingsForm {
    pub fn from_config(c: &Config) -> Self {
        let w = &c.widgets;
        Self {
            clock_enabled: w.clock.enabled,
            clock_refresh: w.clock.refresh_seconds.to_string(),
            clock_seconds: w.clock.show_seconds,
            clock_24h: w.clock.use_24h,
            now_playing_enabled: w.now_playing.enabled,
            now_playing_refresh: w.now_playing.refresh_seconds.to_string(),
            now_playing_album: w.now_playing.show_album,
            now_playing_hide_idle: w.now_playing.hide_when_idle,
            cpu_enabled: w.cpu.enabled,
            cpu_refresh: w.cpu.refresh_seconds.to_string(),
            ram_enabled: w.ram.enabled,
            ram_refresh: w.ram.refresh_seconds.to_string(),
            gpu_enabled: w.gpu.enabled,
            gpu_refresh: w.gpu.refresh_seconds.to_string(),
            fps_enabled: w.fps.enabled,
            fps_refresh: w.fps.refresh_seconds.to_string(),
            theme: c.appearance.theme,
            temperature: c.units.temperature,
            scale_percent: format!("{}", (c.overlay.scale * 100.0).round() as i32),
            opacity_percent: format!("{}", (c.overlay.opacity * 100.0).round() as i32),
            font: c.overlay.font.clone(),
            text_color: c.appearance.text_color.clone(),
            background_color: c.appearance.background_color.clone(),
            accent_color: c.appearance.accent_color.clone(),
            hotkey_toggle: c.hotkeys.toggle.clone(),
            hotkey_edit: c.hotkeys.edit.clone(),
        }
    }

    /// Write the form into `c`. On any invalid number nothing is changed and
    /// every problem is returned. Colours and hotkeys are checked by
    /// `Config::problems` afterwards.
    pub fn apply_to(&self, c: &mut Config) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        let max = MAX_REFRESH_SECONDS;
        let clock = whole_number(
            "The clock's refresh",
            &self.clock_refresh,
            1,
            max,
            &mut errors,
        );
        let np = whole_number(
            "Now Playing's refresh",
            &self.now_playing_refresh,
            1,
            max,
            &mut errors,
        );
        let cpu = whole_number("The CPU refresh", &self.cpu_refresh, 1, max, &mut errors);
        let ram = whole_number("The memory refresh", &self.ram_refresh, 1, max, &mut errors);
        let gpu = whole_number("The GPU refresh", &self.gpu_refresh, 1, max, &mut errors);
        let fps = whole_number("The FPS refresh", &self.fps_refresh, 1, max, &mut errors);
        let scale = whole_number("Scale", &self.scale_percent, 50, 400, &mut errors);
        let opacity = whole_number("Opacity", &self.opacity_percent, 10, 100, &mut errors);
        if self.font.trim().is_empty() {
            errors.push("The font name can't be empty.".into());
        }
        let (
            Some(clock),
            Some(np),
            Some(cpu),
            Some(ram),
            Some(gpu),
            Some(fps),
            Some(scale),
            Some(opacity),
        ) = (clock, np, cpu, ram, gpu, fps, scale, opacity)
        else {
            return Err(errors);
        };
        if !errors.is_empty() {
            return Err(errors);
        }

        let w = &mut c.widgets;
        w.clock.enabled = self.clock_enabled;
        w.clock.refresh_seconds = clock;
        w.clock.show_seconds = self.clock_seconds;
        w.clock.use_24h = self.clock_24h;
        w.now_playing.enabled = self.now_playing_enabled;
        w.now_playing.refresh_seconds = np;
        w.now_playing.show_album = self.now_playing_album;
        w.now_playing.hide_when_idle = self.now_playing_hide_idle;
        w.cpu.enabled = self.cpu_enabled;
        w.cpu.refresh_seconds = cpu;
        w.ram.enabled = self.ram_enabled;
        w.ram.refresh_seconds = ram;
        w.gpu.enabled = self.gpu_enabled;
        w.gpu.refresh_seconds = gpu;
        w.fps.enabled = self.fps_enabled;
        w.fps.refresh_seconds = fps;

        c.appearance.theme = self.theme;
        c.units.temperature = self.temperature;
        c.overlay.scale = scale as f32 / 100.0;
        c.overlay.opacity = opacity as f32 / 100.0;
        c.overlay.font = self.font.trim().to_string();
        c.appearance.text_color = self.text_color.trim().to_string();
        c.appearance.background_color = self.background_color.trim().to_string();
        c.appearance.accent_color = self.accent_color.trim().to_string();

        c.hotkeys.toggle = self.hotkey_toggle.trim().to_string();
        c.hotkeys.edit = self.hotkey_edit.trim().to_string();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_form_of_a_config_applies_back_to_the_same_config() {
        let mut c = Config::default();
        c.overlay.scale = 1.25;
        c.overlay.opacity = 0.8;
        c.appearance.theme = Theme::Light;
        c.units.temperature = TemperatureUnit::Fahrenheit;
        c.widgets.cpu.refresh_seconds = 5;
        c.widgets.now_playing.hide_when_idle = true;
        c.widgets.gpu.enabled = false;
        let form = SettingsForm::from_config(&c);
        let mut back = Config::default();
        form.apply_to(&mut back).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn untouched_settings_outside_the_form_are_kept() {
        let mut c = Config::default();
        c.overlay.offset_x = 99;
        c.overlay.start_visible = false;
        c.layout.insert("clock".into(), Default::default());
        let form = SettingsForm::from_config(&c);
        form.apply_to(&mut c).unwrap();
        assert_eq!(c.overlay.offset_x, 99);
        assert!(!c.overlay.start_visible);
        assert_eq!(c.layout.len(), 1);
    }

    #[test]
    fn bad_numbers_are_all_reported_and_nothing_changes() {
        let original = Config::default();
        let mut c = original.clone();
        let mut form = SettingsForm::from_config(&c);
        form.cpu_refresh = "abc".into();
        form.ram_refresh = "0".into();
        form.gpu_refresh = "61".into();
        form.scale_percent = "49".into();
        form.opacity_percent = "101".into();
        form.font = "  ".into();
        form.clock_enabled = false; // would change if applied
        let errors = form.apply_to(&mut c).unwrap_err();
        assert_eq!(errors.len(), 6, "{errors:?}");
        assert_eq!(c, original, "a failed apply must not change anything");
    }

    #[test]
    fn numbers_accept_surrounding_spaces_but_not_decimals_or_signs() {
        let mut c = Config::default();
        let mut form = SettingsForm::from_config(&c);
        form.cpu_refresh = " 7 ".into();
        form.apply_to(&mut c).unwrap();
        assert_eq!(c.widgets.cpu.refresh_seconds, 7);
        for bad in ["1.5", "-3", "+", "", "1e1"] {
            form.cpu_refresh = bad.into();
            assert!(form.apply_to(&mut c).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn scale_and_opacity_convert_from_percent() {
        let mut c = Config::default();
        let mut form = SettingsForm::from_config(&c);
        form.scale_percent = "150".into();
        form.opacity_percent = "40".into();
        form.apply_to(&mut c).unwrap();
        assert_eq!(c.overlay.scale, 1.5);
        assert!((c.overlay.opacity - 0.4).abs() < 1e-6);
    }

    #[test]
    fn colours_and_hotkeys_are_trimmed_and_left_for_config_problems_to_judge() {
        let mut c = Config::default();
        let mut form = SettingsForm::from_config(&c);
        form.text_color = "  #FF0000 ".into();
        form.hotkey_toggle = " Ctrl+Alt+P ".into();
        form.apply_to(&mut c).unwrap();
        assert_eq!(c.appearance.text_color, "#FF0000");
        assert_eq!(c.hotkeys.toggle, "Ctrl+Alt+P");
        assert!(c.problems().is_empty());
        form.accent_color = "green".into();
        form.apply_to(&mut c).unwrap();
        assert_eq!(c.problems().len(), 1);
    }
}
