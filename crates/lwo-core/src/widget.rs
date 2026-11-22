//! Widget and drawing traits.
//!
//! A widget describes what it shows through the [`Canvas`] trait; each platform
//! implements `Canvas` with its own drawing API. Widgets never touch OS code.

use std::sync::Arc;
use std::time::{Duration, Instant};

/// A data source a widget needs. The platform starts a source only while a
/// visible widget asks for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// The system's current media session (title, artist, position, play state).
    Media,
    /// Overall CPU load.
    Cpu,
    /// Physical memory use.
    Memory,
    /// NVIDIA GPU load, temperature, power and VRAM (through NVML, if installed).
    Gpu,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RamStats {
    pub used_bytes: u64,
    pub total_bytes: u64,
}

/// What the GPU provider chosen at startup can report. Load and VRAM are
/// always available; temperature and power only from vendor libraries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GpuInfo {
    pub has_temperature: bool,
    pub has_power: bool,
}

/// Facts about this machine's data sources, learned once at startup and given
/// to widgets before they are laid out (a widget's size never changes later).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SourceInfo {
    /// `None` when no GPU source works here; the GPU widget is then dropped.
    pub gpu: Option<GpuInfo>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GpuStats {
    /// 0..=100
    pub load_percent: u32,
    /// `None` when the provider can't report it (for example generic counters).
    pub temperature_c: Option<u32>,
    /// `None` on GPUs or drivers that don't report power (common on laptops).
    pub power_watts: Option<f32>,
    pub vram_used_bytes: u64,
    pub vram_total_bytes: u64,
}

/// Latest readings from the polled sources. A field is `None` until its
/// source is running and has produced a value.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stats {
    /// 0.0..=100.0. Needs two samples, so the first poll after start is `None`.
    pub cpu_load_percent: Option<f32>,
    pub ram: Option<RamStats>,
    pub gpu: Option<GpuStats>,
}

/// What the system media session reported at one moment.
#[derive(Clone, Debug, PartialEq)]
pub struct MediaSnapshot {
    pub title: String,
    pub artist: String,
    pub album: String,
    /// Playback position at `at`.
    pub position: Duration,
    /// Zero when unknown (for example a live stream).
    pub duration: Duration,
    pub playing: bool,
    /// When `position` was valid. Between events the widget ticks forward from here.
    pub at: Instant,
}

impl MediaSnapshot {
    /// Position at `now`: advances while playing, never past the end.
    pub fn position_at(&self, now: Instant) -> Duration {
        let mut pos = self.position;
        if self.playing {
            pos += now.saturating_duration_since(self.at);
        }
        if !self.duration.is_zero() {
            pos = pos.min(self.duration);
        }
        pos
    }
}

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
    /// Like `draw_text` but on one line, cut off with "..." if wider than `max_width`.
    #[allow(clippy::too_many_arguments)]
    fn draw_text_trimmed(
        &mut self,
        text: &str,
        font: &str,
        size: f32,
        x: f32,
        y: f32,
        max_width: f32,
        color: Color,
    );
}

/// Data handed to a widget when it updates.
pub struct Context {
    pub time: LocalTime,
    pub now: Instant,
    /// Latest media snapshot, if a media source is running and a session exists.
    pub media: Option<Arc<MediaSnapshot>>,
    pub stats: Stats,
}

impl Context {
    /// A context with no media and no stats.
    pub fn at(time: LocalTime, now: Instant) -> Self {
        Self {
            time,
            now,
            media: None,
            stats: Stats::default(),
        }
    }
}

/// Visual settings shared by all widgets, already multiplied by the user's scale.
#[derive(Clone, Debug)]
pub struct Style {
    pub font: String,
    pub scale: f32,
    pub text: Color,
    pub background: Color,
    /// Highlight colour, for example the progress bar fill.
    pub accent: Color,
}

pub trait Widget {
    /// Stable identifier used in the config file.
    fn id(&self) -> &'static str;
    /// Name shown in the settings UI.
    fn display_name(&self) -> &'static str;
    /// Data sources this widget needs while visible.
    fn sources(&self) -> &'static [Source] {
        &[]
    }
    /// Called once at startup, before `size`, with what the sources can provide.
    fn configure(&mut self, _info: &SourceInfo) {}
    /// How often the widget wants `update` called while visible. May change
    /// with state (for example, slow down when nothing is playing).
    fn refresh_interval(&self) -> Duration;
    /// Refresh internal state. Returns true if what it draws changed.
    fn update(&mut self, ctx: &Context) -> bool;
    /// Largest size the widget will ever draw, in pixels.
    fn size(&self, measure: &dyn TextMeasure, style: &Style) -> (f32, f32);
    /// Draw into `canvas`, which is already cleared and sized to `size`.
    fn render(&self, canvas: &mut dyn Canvas, style: &Style);
}
