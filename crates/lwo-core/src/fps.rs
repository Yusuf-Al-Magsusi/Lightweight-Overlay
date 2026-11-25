//! Frame counting and the FPS widget.
//!
//! `FrameCounter` turns "process X presented a frame at time T" events into a
//! frame rate. It has no OS code, so whatever delivers the events (the overlay
//! reading ETW itself today, or a separate helper process later) can use it.

use std::collections::{HashMap, VecDeque};
use std::time::Duration;

use crate::widget::{Canvas, Context, Source, Style, TextMeasure, Widget};

/// Which kind of event reported a frame. A DirectX app produces both, one for
/// one, so they must never be added together.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameOrigin {
    /// The graphics runtime's own present event (DXGI or Direct3D 9).
    Runtime,
    /// The kernel graphics present event. The only one for some OpenGL and
    /// Vulkan programs.
    Kernel,
}

/// How much history is kept per process.
const HISTORY_SECONDS: u64 = 4;
/// Upper bound on remembered frames per process and origin (about 5000 fps).
const MAX_FRAMES: usize = 20_000;

#[derive(Default)]
struct Series {
    runtime: VecDeque<u64>,
    kernel: VecDeque<u64>,
}

/// Counts frames per process over a sliding window. Times are ticks of any
/// monotonic clock with a known rate.
pub struct FrameCounter {
    ticks_per_second: u64,
    window: u64,
    /// Events arrive in batches a little after they happen, so the window ends
    /// this far in the past. Otherwise the newest frames would be missing and
    /// the rate would read low.
    delay: u64,
    processes: HashMap<u32, Series>,
}

impl FrameCounter {
    pub fn new(ticks_per_second: u64, window: Duration, delay: Duration) -> Self {
        let ticks = |d: Duration| (d.as_secs_f64() * ticks_per_second as f64) as u64;
        Self {
            ticks_per_second: ticks_per_second.max(1),
            window: ticks(window).max(1),
            delay: ticks(delay),
            processes: HashMap::new(),
        }
    }

    pub fn record(&mut self, pid: u32, origin: FrameOrigin, time: u64) {
        let series = self.processes.entry(pid).or_default();
        let queue = match origin {
            FrameOrigin::Runtime => &mut series.runtime,
            FrameOrigin::Kernel => &mut series.kernel,
        };
        if queue.len() >= MAX_FRAMES {
            queue.pop_front();
        }
        queue.push_back(time);
    }

    /// Frames per second of `pid` over the window ending `delay` before `now`,
    /// or `None` if it presented nothing in that window. Runtime events are
    /// used when the process has any; otherwise kernel events.
    pub fn fps(&self, pid: u32, now: u64) -> Option<f32> {
        let series = self.processes.get(&pid)?;
        let end = now.saturating_sub(self.delay);
        let start = end.saturating_sub(self.window);
        let count = |q: &VecDeque<u64>| q.iter().filter(|&&t| t > start && t <= end).count();
        let frames = match count(&series.runtime) {
            0 => count(&series.kernel),
            n => n,
        };
        if frames == 0 {
            return None;
        }
        Some((frames as f64 * self.ticks_per_second as f64 / self.window as f64) as f32)
    }

    /// Forget old frames and processes that stopped presenting.
    pub fn prune(&mut self, now: u64) {
        let oldest = now.saturating_sub(HISTORY_SECONDS * self.ticks_per_second);
        self.processes.retain(|_, s| {
            s.runtime.retain(|&t| t >= oldest);
            s.kernel.retain(|&t| t >= oldest);
            !s.runtime.is_empty() || !s.kernel.is_empty()
        });
    }

    pub fn tracked_processes(&self) -> usize {
        self.processes.len()
    }
}

// --------------------------------------------------------------- widget

const WIDTH: f32 = 110.0;
const PADDING: f32 = 12.0;
const RADIUS: f32 = 10.0;
const GAP: f32 = 4.0;
const LABEL_PX: f32 = 11.0;
const VALUE_PX: f32 = 24.0;

/// Frames per second of the program in the foreground.
pub struct FpsWidget {
    text: String,
}

impl FpsWidget {
    pub fn new() -> Self {
        Self {
            text: String::new(),
        }
    }

    fn lines(measure: &dyn TextMeasure, style: &Style) -> (f32, f32) {
        let line = |px: f32| {
            measure
                .measure_text("Ag", &style.font, px * style.scale)
                .1
                .ceil()
        };
        (line(LABEL_PX), line(VALUE_PX))
    }
}

impl Default for FpsWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for FpsWidget {
    fn id(&self) -> &'static str {
        "fps"
    }

    fn display_name(&self) -> &'static str {
        "FPS"
    }

    fn sources(&self) -> &'static [Source] {
        &[Source::Fps]
    }

    fn refresh_interval(&self) -> Duration {
        Duration::from_secs(1)
    }

    fn update(&mut self, ctx: &Context) -> bool {
        let next = match ctx.stats.fps {
            Some(f) => format!("{:.0}", f.max(0.0)),
            None => "--".to_string(),
        };
        if next == self.text {
            return false;
        }
        self.text = next;
        true
    }

    fn size(&self, measure: &dyn TextMeasure, style: &Style) -> (f32, f32) {
        let (label, value) = Self::lines(measure, style);
        let s = style.scale;
        (WIDTH * s, 2.0 * PADDING * s + label + GAP * 0.5 * s + value)
    }

    fn render(&self, canvas: &mut dyn Canvas, style: &Style) {
        let (w, h) = self.size(&*canvas, style);
        let (label, _) = Self::lines(&*canvas, style);
        let s = style.scale;
        let pad = PADDING * s;
        let mut dim = style.text;
        dim.a *= 0.65;
        canvas.fill_rounded_rect(0.0, 0.0, w, h, RADIUS * s, style.background);
        canvas.draw_text("FPS", &style.font, LABEL_PX * s, pad, pad, dim);
        canvas.draw_text(
            &self.text,
            &style.font,
            VALUE_PX * s,
            pad,
            pad + label + GAP * 0.5 * s,
            style.text,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::{LocalTime, Stats};
    use std::time::Instant;

    /// 1 tick = 1 ms.
    fn counter() -> FrameCounter {
        FrameCounter::new(1000, Duration::from_secs(1), Duration::from_millis(1200))
    }

    /// `rate` frames per second from `from` to `to` milliseconds.
    fn feed(c: &mut FrameCounter, pid: u32, origin: FrameOrigin, rate: u64, from: u64, to: u64) {
        let mut i = 0;
        loop {
            let t = from + i * 1000 / rate;
            if t >= to {
                break;
            }
            c.record(pid, origin, t);
            i += 1;
        }
    }

    #[test]
    fn counts_a_steady_rate() {
        let mut c = counter();
        feed(&mut c, 7, FrameOrigin::Runtime, 120, 0, 10_000);
        let fps = c.fps(7, 10_000).unwrap();
        assert!((fps - 120.0).abs() <= 1.0, "{fps}");
    }

    #[test]
    fn the_window_ends_in_the_past_so_undelivered_events_do_not_lower_the_rate() {
        let mut c = counter();
        // Events up to t=9000 have been delivered, but "now" is 10_000.
        feed(&mut c, 7, FrameOrigin::Runtime, 100, 0, 9_000);
        let fps = c.fps(7, 10_000).unwrap();
        assert!((fps - 100.0).abs() <= 1.0, "{fps}");
    }

    #[test]
    fn runtime_and_kernel_events_are_never_added_together() {
        let mut c = counter();
        feed(&mut c, 7, FrameOrigin::Runtime, 116, 0, 10_000);
        feed(&mut c, 7, FrameOrigin::Kernel, 116, 0, 10_000);
        let fps = c.fps(7, 10_000).unwrap();
        assert!((fps - 116.0).abs() <= 1.0, "double counted: {fps}");
    }

    #[test]
    fn kernel_events_are_used_when_there_are_no_runtime_events() {
        let mut c = counter();
        feed(&mut c, 9, FrameOrigin::Kernel, 60, 0, 10_000);
        let fps = c.fps(9, 10_000).unwrap();
        assert!((fps - 60.0).abs() <= 1.0, "{fps}");
    }

    #[test]
    fn processes_are_counted_separately() {
        let mut c = counter();
        feed(&mut c, 1, FrameOrigin::Runtime, 30, 0, 10_000);
        feed(&mut c, 2, FrameOrigin::Runtime, 240, 0, 10_000);
        assert!((c.fps(1, 10_000).unwrap() - 30.0).abs() <= 1.0);
        assert!((c.fps(2, 10_000).unwrap() - 240.0).abs() <= 1.0);
        assert_eq!(c.fps(3, 10_000), None);
    }

    #[test]
    fn a_program_that_stops_presenting_reads_as_nothing_not_as_its_old_rate() {
        let mut c = counter();
        feed(&mut c, 7, FrameOrigin::Runtime, 100, 0, 5_000);
        assert!(c.fps(7, 6_000).is_some());
        assert_eq!(c.fps(7, 9_000), None);
    }

    #[test]
    fn low_rates_are_reported() {
        let mut c = counter();
        feed(&mut c, 7, FrameOrigin::Runtime, 2, 0, 10_000);
        let fps = c.fps(7, 10_000).unwrap();
        assert!((1.0..=3.0).contains(&fps), "{fps}");
    }

    #[test]
    fn pruning_forgets_old_frames_and_idle_processes() {
        let mut c = counter();
        feed(&mut c, 1, FrameOrigin::Runtime, 60, 0, 1_000);
        feed(&mut c, 2, FrameOrigin::Kernel, 60, 0, 20_000);
        c.prune(20_000);
        assert_eq!(c.tracked_processes(), 1);
        assert!(c.fps(2, 20_000).is_some());
    }

    #[test]
    fn memory_stays_bounded_under_an_absurd_rate() {
        let mut c = counter();
        for i in 0..100_000u64 {
            c.record(1, FrameOrigin::Runtime, i / 100);
        }
        assert!(c.processes[&1].runtime.len() <= MAX_FRAMES);
    }

    fn ctx(fps: Option<f32>) -> Context {
        Context {
            stats: Stats {
                fps,
                ..Stats::default()
            },
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

    #[test]
    fn widget_shows_whole_frames_and_redraws_only_when_the_number_changes() {
        let mut w = FpsWidget::new();
        assert!(w.update(&ctx(None)));
        assert_eq!(w.text, "--");
        assert!(w.update(&ctx(Some(116.9))));
        assert_eq!(w.text, "117");
        assert!(!w.update(&ctx(Some(117.2))));
        assert!(w.update(&ctx(Some(118.0))));
        assert!(w.update(&ctx(None)));
    }

    #[test]
    fn widget_declares_the_fps_source() {
        assert_eq!(FpsWidget::new().sources(), &[Source::Fps]);
    }
}
