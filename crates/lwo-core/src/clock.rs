//! Clock widget.

use std::time::Duration;

use crate::widget::{Canvas, Context, LocalTime, Style, TextMeasure, Widget};

/// Base font size in pixels at scale 1.0.
const FONT_PX: f32 = 34.0;
const PADDING: f32 = 12.0;
const RADIUS: f32 = 10.0;

pub struct Clock {
    pub show_seconds: bool,
    pub use_24h: bool,
    text: String,
}

impl Clock {
    pub fn new(show_seconds: bool, use_24h: bool) -> Self {
        Self {
            show_seconds,
            use_24h,
            text: String::new(),
        }
    }

    pub fn format(&self, t: LocalTime) -> String {
        let (hour, suffix) = if self.use_24h {
            (t.hour, "")
        } else {
            let h = match t.hour % 12 {
                0 => 12,
                h => h,
            };
            (h, if t.hour < 12 { " AM" } else { " PM" })
        };
        if self.show_seconds {
            format!("{hour:02}:{:02}:{:02}{suffix}", t.minute, t.second)
        } else {
            format!("{hour:02}:{:02}{suffix}", t.minute)
        }
    }
}

impl Widget for Clock {
    fn id(&self) -> &'static str {
        "clock"
    }

    fn display_name(&self) -> &'static str {
        "Clock"
    }

    fn refresh_interval(&self) -> Duration {
        Duration::from_secs(1)
    }

    fn update(&mut self, ctx: &Context) -> bool {
        let next = self.format(ctx.time);
        if next == self.text {
            return false;
        }
        self.text = next;
        true
    }

    fn size(&self, measure: &dyn TextMeasure, style: &Style) -> (f32, f32) {
        // Widest possible string: every digit is 8, PM suffix if 12-hour.
        let widest = self.format(LocalTime {
            hour: if self.use_24h { 20 } else { 8 },
            minute: 8,
            second: 8,
        });
        let widest = if self.use_24h {
            widest
        } else {
            widest.replace("AM", "PM")
        };
        let (w, h) = measure.measure_text(&widest, &style.font, FONT_PX * style.scale);
        let pad = PADDING * style.scale;
        (w.ceil() + 2.0 * pad, h.ceil() + 2.0 * pad)
    }

    fn render(&self, canvas: &mut dyn Canvas, style: &Style) {
        let (w, h) = self.size(&*canvas, style);
        let pad = PADDING * style.scale;
        canvas.fill_rounded_rect(0.0, 0.0, w, h, RADIUS * style.scale, style.background);
        canvas.draw_text(
            &self.text,
            &style.font,
            FONT_PX * style.scale,
            pad,
            pad,
            style.text,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(hour: u8, minute: u8, second: u8) -> LocalTime {
        LocalTime {
            hour,
            minute,
            second,
        }
    }

    #[test]
    fn formats_24h_with_seconds() {
        assert_eq!(Clock::new(true, true).format(t(7, 5, 9)), "07:05:09");
    }

    #[test]
    fn formats_24h_without_seconds() {
        assert_eq!(Clock::new(false, true).format(t(23, 59, 1)), "23:59");
    }

    #[test]
    fn formats_12h() {
        let c = Clock::new(true, false);
        assert_eq!(c.format(t(0, 0, 0)), "12:00:00 AM");
        assert_eq!(c.format(t(12, 30, 0)), "12:30:00 PM");
        assert_eq!(c.format(t(15, 4, 5)), "03:04:05 PM");
    }

    fn ctx(time: LocalTime) -> Context {
        Context::at(time, std::time::Instant::now())
    }

    #[test]
    fn update_reports_change_only_when_text_changes() {
        let mut c = Clock::new(true, true);
        assert!(c.update(&ctx(t(1, 2, 3))));
        assert!(!c.update(&ctx(t(1, 2, 3))));
        assert!(c.update(&ctx(t(1, 2, 4))));
    }
}
