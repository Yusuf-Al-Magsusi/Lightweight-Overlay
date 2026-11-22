//! Widget and drawing traits.
//!
//! A widget describes what it shows through the [`Canvas`] trait; each platform
//! implements `Canvas` with its own drawing API. Widgets never touch OS code.

use std::time::Duration;

/// Local wall-clock time, supplied by the platform layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalTime {
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

/// RGBA colour, each channel 0.0..=1.0, not premultiplied.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }
}

/// Measures text without drawing it.
pub trait TextMeasure {
    /// Width and height in pixels of `text` at `size` px using `font`.
    fn measure_text(&self, text: &str, font: &str, size: f32) -> (f32, f32);
}

/// Minimal drawing surface. Coordinates are pixels, origin top-left.
pub trait Canvas: TextMeasure {
    fn fill_rounded_rect(&mut self, x: f32, y: f32, w: f32, h: f32, radius: f32, color: Color);
    fn draw_text(&mut self, text: &str, font: &str, size: f32, x: f32, y: f32, color: Color);
}

/// Data handed to a widget when it updates.
pub struct Context {
    pub time: LocalTime,
}

/// Visual settings shared by all widgets, already multiplied by the user's scale.
#[derive(Clone, Debug)]
pub struct Style {
    pub font: String,
    pub scale: f32,
    pub text: Color,
    pub background: Color,
}

pub trait Widget {
    /// Stable identifier used in the config file.
    fn id(&self) -> &'static str;
    /// Name shown in the settings UI.
    fn display_name(&self) -> &'static str;
    /// How often the widget wants `update` called while visible.
    fn refresh_interval(&self) -> Duration;
    /// Refresh internal state. Returns true if what it draws changed.
    fn update(&mut self, ctx: &Context) -> bool;
    /// Largest size the widget will ever draw, in pixels.
    fn size(&self, measure: &dyn TextMeasure, style: &Style) -> (f32, f32);
    /// Draw into `canvas`, which is already cleared and sized to `size`.
    fn render(&self, canvas: &mut dyn Canvas, style: &Style);
}
