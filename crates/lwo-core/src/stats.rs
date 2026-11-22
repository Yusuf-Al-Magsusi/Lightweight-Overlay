//! CPU, memory and GPU widgets. They share one compact panel layout: a dim
//! label, a large value, an optional bar, and optional detail lines.
//!
//! Each widget redraws only when its formatted text or whole-percent bar changes.

use std::time::Duration;

use crate::widget::{
    Canvas, Color, Context, GpuInfo, GpuStats, Source, SourceInfo, Style, TextMeasure, Widget,
};

const PADDING: f32 = 12.0;
const RADIUS: f32 = 10.0;
const GAP: f32 = 4.0;
const LABEL_PX: f32 = 11.0;
const VALUE_PX: f32 = 24.0;
const DETAIL_PX: f32 = 13.0;
const BAR_H: f32 = 6.0;

/// Placeholder shown while a value is not available yet.
const NA: &str = "--";

/// Bytes as gigabytes with one decimal, using 1024-based units like Task Manager.
pub fn format_gb(bytes: u64) -> String {
    format!("{:.1}", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
}

/// What a panel shows right now. Compared to decide whether to redraw.
#[derive(Clone, Debug, Default, PartialEq)]
struct View {
    headline: String,
    /// Whole percent, 0..=100.
    bar: Option<u8>,
    details: Vec<String>,
}

/// Fixed shape of a panel, so its size never changes.
struct Shape {
    label: &'static str,
    width: f32,
    has_bar: bool,
    detail_lines: usize,
}

struct Layout {
    w: f32,
    h: f32,
    pad: f32,
    inner_w: f32,
    label_y: f32,
    value_y: f32,
    bar_y: f32,
    detail_y: f32,
    detail_step: f32,
}

fn layout(shape: &Shape, measure: &dyn TextMeasure, style: &Style) -> Layout {
    let s = style.scale;
    let line = |px: f32| measure.measure_text("Ag", &style.font, px * s).1.ceil();
    let (pad, gap) = (PADDING * s, GAP * s);
    let label_y = pad;
    let value_y = label_y + line(LABEL_PX) + gap * 0.5;
    let mut y = value_y + line(VALUE_PX) + gap;
    let bar_y = y;
    if shape.has_bar {
        y += BAR_H * s + gap;
    }
    let detail_y = y;
    let detail_step = line(DETAIL_PX);
    y += detail_step * shape.detail_lines as f32;
    Layout {
        w: shape.width * s,
        h: y + pad
            - if shape.detail_lines == 0 && !shape.has_bar {
                gap
            } else {
                0.0
            },
        pad,
        inner_w: shape.width * s - 2.0 * pad,
        label_y,
        value_y,
        bar_y,
        detail_y,
        detail_step,
    }
}

fn draw(shape: &Shape, view: &View, canvas: &mut dyn Canvas, style: &Style) {
    let l = layout(shape, &*canvas, style);
    let s = style.scale;
    let font = style.font.as_str();
    let dim = Color::new(
        style.text.r,
        style.text.g,
        style.text.b,
        style.text.a * 0.65,
    );

    canvas.fill_rounded_rect(0.0, 0.0, l.w, l.h, RADIUS * s, style.background);
    canvas.draw_text(shape.label, font, LABEL_PX * s, l.pad, l.label_y, dim);
    canvas.draw_text_trimmed(
        &view.headline,
        font,
        VALUE_PX * s,
        l.pad,
        l.value_y,
        l.inner_w,
        style.text,
    );
    if shape.has_bar {
        let bar_h = BAR_H * s;
        let r = bar_h / 2.0;
        canvas.fill_rounded_rect(
            l.pad,
            l.bar_y,
            l.inner_w,
            bar_h,
            r,
            Color::new(style.text.r, style.text.g, style.text.b, 0.2),
        );
        if let Some(pct) = view.bar {
            let fill = l.inner_w * f32::from(pct) / 100.0;
            if fill > 0.0 {
                canvas.fill_rounded_rect(l.pad, l.bar_y, fill, bar_h, r, style.accent);
            }
        }
    }
    for (i, text) in view.details.iter().enumerate() {
        canvas.draw_text_trimmed(
            text,
            font,
            DETAIL_PX * s,
            l.pad,
            l.detail_y + l.detail_step * i as f32,
            l.inner_w,
            dim,
        );
    }
}

fn percent(p: f32) -> u8 {
    p.round().clamp(0.0, 100.0) as u8
}

// ---------------------------------------------------------------- CPU

pub struct CpuWidget {
    view: View,
}

const CPU_SHAPE: Shape = Shape {
    label: "CPU",
    width: 150.0,
    has_bar: true,
    detail_lines: 0,
};

impl CpuWidget {
    pub fn new() -> Self {
        Self {
            view: View::default(),
        }
    }

    fn view_for(ctx: &Context) -> View {
        match ctx.stats.cpu_load_percent {
            Some(p) => View {
                headline: format!("{}%", percent(p)),
                bar: Some(percent(p)),
                details: vec![],
            },
            None => View {
                headline: format!("{NA}%"),
                bar: None,
                details: vec![],
            },
        }
    }
}

impl Default for CpuWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for CpuWidget {
    fn id(&self) -> &'static str {
        "cpu"
    }
    fn display_name(&self) -> &'static str {
        "CPU load"
    }
    fn sources(&self) -> &'static [Source] {
        &[Source::Cpu]
    }
    fn refresh_interval(&self) -> Duration {
        Duration::from_secs(1)
    }
    fn update(&mut self, ctx: &Context) -> bool {
        let next = Self::view_for(ctx);
        if next == self.view {
            return false;
        }
        self.view = next;
        true
    }
    fn size(&self, measure: &dyn TextMeasure, style: &Style) -> (f32, f32) {
        let l = layout(&CPU_SHAPE, measure, style);
        (l.w, l.h)
    }
    fn render(&self, canvas: &mut dyn Canvas, style: &Style) {
        draw(&CPU_SHAPE, &self.view, canvas, style);
    }
}

// ---------------------------------------------------------------- RAM

pub struct RamWidget {
    view: View,
}

const RAM_SHAPE: Shape = Shape {
    label: "MEMORY",
    width: 200.0,
    has_bar: true,
    detail_lines: 0,
};

impl RamWidget {
    pub fn new() -> Self {
        Self {
            view: View::default(),
        }
    }

    fn view_for(ctx: &Context) -> View {
        match ctx.stats.ram {
            Some(r) if r.total_bytes > 0 => View {
                headline: format!(
                    "{} / {} GB",
                    format_gb(r.used_bytes),
                    format_gb(r.total_bytes)
                ),
                bar: Some(percent(r.used_bytes as f32 / r.total_bytes as f32 * 100.0)),
                details: vec![],
            },
            _ => View {
                headline: format!("{NA} GB"),
                bar: None,
                details: vec![],
            },
        }
    }
}

impl Default for RamWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for RamWidget {
    fn id(&self) -> &'static str {
        "ram"
    }
    fn display_name(&self) -> &'static str {
        "Memory"
    }
    fn sources(&self) -> &'static [Source] {
        &[Source::Memory]
    }
    fn refresh_interval(&self) -> Duration {
        Duration::from_secs(1)
    }
    fn update(&mut self, ctx: &Context) -> bool {
        let next = Self::view_for(ctx);
        if next == self.view {
            return false;
        }
        self.view = next;
        true
    }
    fn size(&self, measure: &dyn TextMeasure, style: &Style) -> (f32, f32) {
        let l = layout(&RAM_SHAPE, measure, style);
        (l.w, l.h)
    }
    fn render(&self, canvas: &mut dyn Canvas, style: &Style) {
        draw(&RAM_SHAPE, &self.view, canvas, style);
    }
}

// ---------------------------------------------------------------- GPU

/// GPU panel. Shows load and VRAM from any provider; the temperature / power
/// row appears only if the provider chosen at startup can report them
/// (NVIDIA through NVML can; the generic Windows counters can't).
pub struct GpuWidget {
    info: GpuInfo,
    view: View,
}

impl GpuWidget {
    pub fn new() -> Self {
        Self {
            // Until `configure` runs, assume the richest provider.
            info: GpuInfo {
                has_temperature: true,
                has_power: true,
            },
            view: View::default(),
        }
    }

    fn shape(&self) -> Shape {
        let vendor_row = self.info.has_temperature || self.info.has_power;
        Shape {
            label: "GPU",
            width: 220.0,
            has_bar: true,
            detail_lines: 1 + usize::from(vendor_row),
        }
    }

    /// "62 C    180 W", with only the parts this provider can report.
    fn vendor_line(&self, g: Option<&GpuStats>) -> String {
        let mut parts = Vec::new();
        if self.info.has_temperature {
            let t = g
                .and_then(|g| g.temperature_c)
                .map_or_else(|| NA.to_string(), |t| t.to_string());
            parts.push(format!("{t} \u{00B0}C"));
        }
        if self.info.has_power {
            let w = g
                .and_then(|g| g.power_watts)
                .map_or_else(|| NA.to_string(), |w| format!("{w:.0}"));
            parts.push(format!("{w} W"));
        }
        parts.join("    ")
    }

    fn view_for(&self, ctx: &Context) -> View {
        let vendor_row = self.info.has_temperature || self.info.has_power;
        let g = ctx.stats.gpu.as_ref();
        let mut details = Vec::new();
        if vendor_row {
            details.push(self.vendor_line(g));
        }
        match g {
            Some(g) => {
                details.push(format!(
                    "VRAM {} / {} GB",
                    format_gb(g.vram_used_bytes),
                    format_gb(g.vram_total_bytes)
                ));
                View {
                    headline: format!("{}%", g.load_percent.min(100)),
                    bar: Some(g.load_percent.min(100) as u8),
                    details,
                }
            }
            None => {
                details.push(format!("VRAM {NA}"));
                View {
                    headline: format!("{NA}%"),
                    bar: None,
                    details,
                }
            }
        }
    }
}

impl Default for GpuWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for GpuWidget {
    fn id(&self) -> &'static str {
        "gpu"
    }
    fn display_name(&self) -> &'static str {
        "GPU"
    }
    fn sources(&self) -> &'static [Source] {
        &[Source::Gpu]
    }
    fn configure(&mut self, info: &SourceInfo) {
        if let Some(gpu) = info.gpu {
            self.info = gpu;
        }
    }
    fn refresh_interval(&self) -> Duration {
        Duration::from_secs(1)
    }
    fn update(&mut self, ctx: &Context) -> bool {
        let next = self.view_for(ctx);
        if next == self.view {
            return false;
        }
        self.view = next;
        true
    }
    fn size(&self, measure: &dyn TextMeasure, style: &Style) -> (f32, f32) {
        let l = layout(&self.shape(), measure, style);
        (l.w, l.h)
    }
    fn render(&self, canvas: &mut dyn Canvas, style: &Style) {
        draw(&self.shape(), &self.view, canvas, style);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::{LocalTime, RamStats, Stats};
    use std::time::Instant;

    fn ctx(stats: Stats) -> Context {
        Context {
            stats,
            ..Context::at(
                LocalTime {
                    hour: 0,
                    minute: 0,
                    second: 0,
                },
                Instant::now(),
            )
        }
    }

    const GIB: u64 = 1024 * 1024 * 1024;

    #[test]
    fn formats_gigabytes_with_one_decimal() {
        assert_eq!(format_gb(0), "0.0");
        assert_eq!(format_gb(GIB), "1.0");
        assert_eq!(format_gb(GIB * 3 / 2), "1.5");
        assert_eq!(format_gb(16 * GIB), "16.0");
    }

    #[test]
    fn cpu_shows_dashes_until_a_value_arrives_then_redraws_only_on_whole_percent_change() {
        let mut w = CpuWidget::new();
        assert!(w.update(&ctx(Stats::default())));
        assert_eq!(w.view.headline, "--%");
        assert!(!w.update(&ctx(Stats::default())));

        let with = |p: f32| {
            ctx(Stats {
                cpu_load_percent: Some(p),
                ..Stats::default()
            })
        };
        assert!(w.update(&with(23.4)));
        assert_eq!(w.view.headline, "23%");
        assert!(
            !w.update(&with(22.6)),
            "22.6 rounds to 23, nothing visible changed"
        );
        assert!(w.update(&with(24.0)));
    }

    #[test]
    fn cpu_percent_is_clamped() {
        let mut w = CpuWidget::new();
        w.update(&ctx(Stats {
            cpu_load_percent: Some(180.0),
            ..Stats::default()
        }));
        assert_eq!(w.view.headline, "100%");
        w.update(&ctx(Stats {
            cpu_load_percent: Some(-5.0),
            ..Stats::default()
        }));
        assert_eq!(w.view.headline, "0%");
    }

    #[test]
    fn ram_shows_used_over_total_and_percent_bar() {
        let mut w = RamWidget::new();
        w.update(&ctx(Stats {
            ram: Some(RamStats {
                used_bytes: 8 * GIB,
                total_bytes: 32 * GIB,
            }),
            ..Stats::default()
        }));
        assert_eq!(w.view.headline, "8.0 / 32.0 GB");
        assert_eq!(w.view.bar, Some(25));
    }

    #[test]
    fn ram_with_zero_total_shows_dashes_instead_of_dividing_by_zero() {
        let mut w = RamWidget::new();
        w.update(&ctx(Stats {
            ram: Some(RamStats {
                used_bytes: 1,
                total_bytes: 0,
            }),
            ..Stats::default()
        }));
        assert_eq!(w.view.headline, "-- GB");
        assert_eq!(w.view.bar, None);
    }

    #[test]
    fn gpu_formats_all_fields() {
        let mut w = GpuWidget::new();
        w.update(&ctx(Stats {
            gpu: Some(GpuStats {
                load_percent: 45,
                temperature_c: Some(62),
                power_watts: Some(180.4),
                vram_used_bytes: 4 * GIB,
                vram_total_bytes: 16 * GIB,
            }),
            ..Stats::default()
        }));
        assert_eq!(w.view.headline, "45%");
        assert_eq!(w.view.bar, Some(45));
        assert_eq!(w.view.details[0], "62 \u{00B0}C    180 W");
        assert_eq!(w.view.details[1], "VRAM 4.0 / 16.0 GB");
    }

    #[test]
    fn gpu_without_power_reading_shows_dashes_for_power_only() {
        let mut w = GpuWidget::new();
        w.update(&ctx(Stats {
            gpu: Some(GpuStats {
                load_percent: 10,
                temperature_c: Some(40),
                power_watts: None,
                vram_used_bytes: GIB,
                vram_total_bytes: 8 * GIB,
            }),
            ..Stats::default()
        }));
        assert_eq!(w.view.details[0], "40 \u{00B0}C    -- W");
    }

    /// Measures every string as 8 px per character, 20 px tall.
    struct FixedMeasure;
    impl TextMeasure for FixedMeasure {
        fn measure_text(&self, text: &str, _font: &str, _size: f32) -> (f32, f32) {
            (text.chars().count() as f32 * 8.0, 20.0)
        }
    }

    fn test_style() -> Style {
        Style {
            font: "Test".into(),
            scale: 1.0,
            text: Color::new(1.0, 1.0, 1.0, 1.0),
            background: Color::new(0.0, 0.0, 0.0, 0.5),
            accent: Color::new(0.0, 1.0, 0.0, 1.0),
        }
    }

    fn sample_gpu() -> GpuStats {
        GpuStats {
            load_percent: 30,
            temperature_c: None,
            power_watts: None,
            vram_used_bytes: 2 * GIB,
            vram_total_bytes: 16 * GIB,
        }
    }

    #[test]
    fn generic_gpu_provider_shows_load_and_vram_only() {
        let mut w = GpuWidget::new();
        w.configure(&SourceInfo {
            gpu: Some(GpuInfo {
                has_temperature: false,
                has_power: false,
            }),
        });
        w.update(&ctx(Stats {
            gpu: Some(sample_gpu()),
            ..Stats::default()
        }));
        assert_eq!(w.view.headline, "30%");
        assert_eq!(w.view.details, vec!["VRAM 2.0 / 16.0 GB".to_string()]);
    }

    #[test]
    fn panel_without_vendor_row_is_shorter() {
        let style = test_style();
        let full = GpuWidget::new();
        let mut generic = GpuWidget::new();
        generic.configure(&SourceInfo {
            gpu: Some(GpuInfo::default()),
        });
        let (wf, hf) = full.size(&FixedMeasure, &style);
        let (wg, hg) = generic.size(&FixedMeasure, &style);
        assert_eq!(wf, wg, "same width");
        assert!(hg < hf, "generic panel must be shorter: {hg} vs {hf}");
    }

    #[test]
    fn temperature_only_provider_shows_just_that_part() {
        let mut w = GpuWidget::new();
        w.configure(&SourceInfo {
            gpu: Some(GpuInfo {
                has_temperature: true,
                has_power: false,
            }),
        });
        let mut g = sample_gpu();
        g.temperature_c = Some(55);
        w.update(&ctx(Stats {
            gpu: Some(g),
            ..Stats::default()
        }));
        assert_eq!(w.view.details[0], "55 \u{00B0}C");
    }

    #[test]
    fn configure_without_a_gpu_keeps_defaults() {
        let mut w = GpuWidget::new();
        w.configure(&SourceInfo { gpu: None });
        assert!(w.info.has_temperature && w.info.has_power);
    }

    #[test]
    fn gpu_without_readings_shows_dashes() {
        let mut w = GpuWidget::new();
        w.update(&ctx(Stats::default()));
        assert_eq!(w.view.headline, "--%");
        assert_eq!(w.view.bar, None);
    }

    #[test]
    fn widgets_declare_their_sources() {
        assert_eq!(CpuWidget::new().sources(), &[Source::Cpu]);
        assert_eq!(RamWidget::new().sources(), &[Source::Memory]);
        assert_eq!(GpuWidget::new().sources(), &[Source::Gpu]);
    }
}
