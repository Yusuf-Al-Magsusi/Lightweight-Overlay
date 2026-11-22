//! Now Playing widget: state label, title, artist, progress bar and times.
//!
//! The media snapshot arrives from the platform only when something changes.
//! Between those events this widget advances the position itself.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::widget::{Canvas, Color, Context, MediaSnapshot, Source, Style, TextMeasure, Widget};

const WIDTH: f32 = 300.0;
const PADDING: f32 = 12.0;
const RADIUS: f32 = 10.0;
const GAP: f32 = 4.0;
const LABEL_PX: f32 = 11.0;
const TITLE_PX: f32 = 18.0;
const ARTIST_PX: f32 = 14.0;
const TIME_PX: f32 = 12.0;
const BAR_H: f32 = 6.0;

/// Refresh rate while paused or idle. Media events still redraw immediately.
const IDLE_REFRESH: Duration = Duration::from_secs(3600);

/// What was last drawn, to tell whether anything visible changed.
#[derive(PartialEq)]
struct Shown {
    title: String,
    artist: String,
    album: String,
    playing: bool,
    pos_secs: u64,
    dur_secs: u64,
}

pub struct NowPlaying {
    pub show_album: bool,
    /// Hide the whole panel while there is no media session at all.
    pub hide_when_idle: bool,
    snap: Option<Arc<MediaSnapshot>>,
    now: Instant,
    shown: Option<Shown>,
}

/// `m:ss`, or `h:mm:ss` from one hour up.
pub fn format_time(secs: u64) -> String {
    let (h, m, s) = (secs / 3600, (secs / 60) % 60, secs % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

struct Layout {
    w: f32,
    h: f32,
    pad: f32,
    inner_w: f32,
    label_y: f32,
    title_y: f32,
    artist_y: f32,
    bar_y: f32,
    times_y: f32,
}

impl NowPlaying {
    pub fn new(show_album: bool, hide_when_idle: bool) -> Self {
        Self {
            show_album,
            hide_when_idle,
            snap: None,
            now: Instant::now(),
            shown: None,
        }
    }

    fn layout(measure: &dyn TextMeasure, style: &Style) -> Layout {
        let s = style.scale;
        let line = |px: f32| measure.measure_text("Ag", &style.font, px * s).1.ceil();
        let (pad, gap) = (PADDING * s, GAP * s);
        let label_y = pad;
        let title_y = label_y + line(LABEL_PX) + gap;
        let artist_y = title_y + line(TITLE_PX);
        let bar_y = artist_y + line(ARTIST_PX) + gap * 2.0;
        let times_y = bar_y + BAR_H * s + gap;
        Layout {
            w: WIDTH * s,
            h: times_y + line(TIME_PX) + pad,
            pad,
            inner_w: WIDTH * s - 2.0 * pad,
            label_y,
            title_y,
            artist_y,
            bar_y,
            times_y,
        }
    }

    fn matches(&self, snap: &Option<Arc<MediaSnapshot>>, now: Instant) -> bool {
        let shown = match &self.shown {
            Some(s) => s,
            None => return false,
        };
        match snap {
            None => shown.title.is_empty() && shown.dur_secs == 0 && !shown.playing,
            Some(m) => {
                shown.title == m.title
                    && shown.artist == m.artist
                    && shown.album == m.album
                    && shown.playing == m.playing
                    && shown.pos_secs == m.position_at(now).as_secs()
                    && shown.dur_secs == m.duration.as_secs()
            }
        }
    }
}

fn dim(c: Color) -> Color {
    Color::new(c.r, c.g, c.b, c.a * 0.65)
}

impl Widget for NowPlaying {
    fn id(&self) -> &'static str {
        "now_playing"
    }

    fn display_name(&self) -> &'static str {
        "Now Playing"
    }

    fn sources(&self) -> &'static [Source] {
        &[Source::Media]
    }

    fn visible(&self) -> bool {
        // Paused still counts as a session; only a missing one hides the panel.
        !(self.hide_when_idle && self.snap.is_none())
    }

    fn refresh_interval(&self) -> Duration {
        match &self.snap {
            Some(m) if m.playing => Duration::from_secs(1),
            _ => IDLE_REFRESH,
        }
    }

    fn update(&mut self, ctx: &Context) -> bool {
        self.now = ctx.now;
        if self.matches(&ctx.media, ctx.now) {
            // Keep the newest snapshot so the next position is computed from it.
            self.snap = ctx.media.clone();
            return false;
        }
        self.snap = ctx.media.clone();
        self.shown = Some(match &self.snap {
            None => Shown {
                title: String::new(),
                artist: String::new(),
                album: String::new(),
                playing: false,
                pos_secs: 0,
                dur_secs: 0,
            },
            Some(m) => Shown {
                title: m.title.clone(),
                artist: m.artist.clone(),
                album: m.album.clone(),
                playing: m.playing,
                pos_secs: m.position_at(ctx.now).as_secs(),
                dur_secs: m.duration.as_secs(),
            },
        });
        true
    }

    fn size(&self, measure: &dyn TextMeasure, style: &Style) -> (f32, f32) {
        let l = Self::layout(measure, style);
        (l.w, l.h)
    }

    fn render(&self, canvas: &mut dyn Canvas, style: &Style) {
        let l = Self::layout(&*canvas, style);
        let s = style.scale;
        let font = style.font.as_str();
        canvas.fill_rounded_rect(0.0, 0.0, l.w, l.h, RADIUS * s, style.background);

        let (label, title, artist, pos, dur) = match &self.snap {
            None => (
                "NOW PLAYING",
                "Nothing playing".to_string(),
                String::new(),
                0,
                0,
            ),
            Some(m) => {
                let mut artist = m.artist.clone();
                if self.show_album && !m.album.is_empty() {
                    if !artist.is_empty() {
                        artist.push_str(" - ");
                    }
                    artist.push_str(&m.album);
                }
                (
                    if m.playing { "NOW PLAYING" } else { "PAUSED" },
                    m.title.clone(),
                    artist,
                    m.position_at(self.now).as_secs(),
                    m.duration.as_secs(),
                )
            }
        };
        let idle = self.snap.is_none();

        canvas.draw_text(label, font, LABEL_PX * s, l.pad, l.label_y, dim(style.text));
        canvas.draw_text_trimmed(
            &title,
            font,
            TITLE_PX * s,
            l.pad,
            l.title_y,
            l.inner_w,
            if idle { dim(style.text) } else { style.text },
        );
        canvas.draw_text_trimmed(
            &artist,
            font,
            ARTIST_PX * s,
            l.pad,
            l.artist_y,
            l.inner_w,
            dim(style.text),
        );

        // Progress bar: track, then fill.
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
        if dur > 0 {
            let ratio = (pos as f32 / dur as f32).clamp(0.0, 1.0);
            let fill_w = l.inner_w * ratio;
            if fill_w > 0.0 {
                canvas.fill_rounded_rect(l.pad, l.bar_y, fill_w, bar_h, r, style.accent);
            }
        }

        // Times: elapsed on the left, total on the right.
        let time_px = TIME_PX * s;
        canvas.draw_text(
            &format_time(pos),
            font,
            time_px,
            l.pad,
            l.times_y,
            dim(style.text),
        );
        let total = if dur > 0 {
            format_time(dur)
        } else {
            "--:--".to_string()
        };
        let (tw, _) = canvas.measure_text(&total, font, time_px);
        canvas.draw_text(
            &total,
            font,
            time_px,
            l.pad + l.inner_w - tw,
            l.times_y,
            dim(style.text),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::LocalTime;

    fn snap(playing: bool, pos: u64, dur: u64, at: Instant) -> Arc<MediaSnapshot> {
        Arc::new(MediaSnapshot {
            title: "Song".into(),
            artist: "Artist".into(),
            album: "Album".into(),
            position: Duration::from_secs(pos),
            duration: Duration::from_secs(dur),
            playing,
            at,
        })
    }

    fn ctx(now: Instant, media: Option<Arc<MediaSnapshot>>) -> Context {
        Context {
            media,
            ..Context::at(
                LocalTime {
                    hour: 0,
                    minute: 0,
                    second: 0,
                },
                now,
            )
        }
    }

    #[test]
    fn formats_times() {
        assert_eq!(format_time(0), "0:00");
        assert_eq!(format_time(65), "1:05");
        assert_eq!(format_time(3599), "59:59");
        assert_eq!(format_time(3600), "1:00:00");
        assert_eq!(format_time(3725), "1:02:05");
    }

    #[test]
    fn position_advances_only_while_playing_and_stops_at_end() {
        let t0 = Instant::now();
        let later = t0 + Duration::from_secs(10);
        assert_eq!(snap(true, 5, 100, t0).position_at(later).as_secs(), 15);
        assert_eq!(snap(false, 5, 100, t0).position_at(later).as_secs(), 5);
        assert_eq!(snap(true, 95, 100, t0).position_at(later).as_secs(), 100);
        // Unknown duration (live stream): no clamping.
        assert_eq!(snap(true, 5, 0, t0).position_at(later).as_secs(), 15);
    }

    #[test]
    fn first_update_draws_even_with_no_media() {
        let mut w = NowPlaying::new(false, false);
        let t0 = Instant::now();
        assert!(w.update(&ctx(t0, None)));
        assert!(!w.update(&ctx(t0, None)));
    }

    #[test]
    fn redraws_each_second_while_playing_but_not_within_a_second() {
        let mut w = NowPlaying::new(false, false);
        let t0 = Instant::now();
        let m = snap(true, 10, 200, t0);
        assert!(w.update(&ctx(t0, Some(m.clone()))));
        assert!(!w.update(&ctx(t0 + Duration::from_millis(400), Some(m.clone()))));
        assert!(w.update(&ctx(t0 + Duration::from_millis(1100), Some(m))));
    }

    #[test]
    fn paused_does_not_redraw_as_time_passes() {
        let mut w = NowPlaying::new(false, false);
        let t0 = Instant::now();
        let m = snap(false, 10, 200, t0);
        assert!(w.update(&ctx(t0, Some(m.clone()))));
        assert!(!w.update(&ctx(t0 + Duration::from_secs(30), Some(m))));
    }

    #[test]
    fn track_and_state_changes_redraw() {
        let mut w = NowPlaying::new(false, false);
        let t0 = Instant::now();
        assert!(w.update(&ctx(t0, Some(snap(true, 10, 200, t0)))));
        // Pause at the same position.
        assert!(w.update(&ctx(t0, Some(snap(false, 10, 200, t0)))));
        // New track.
        let mut other = (*snap(false, 10, 200, t0)).clone();
        other.title = "Other".into();
        assert!(w.update(&ctx(t0, Some(Arc::new(other)))));
        // Media goes away.
        assert!(w.update(&ctx(t0, None)));
    }

    #[test]
    fn refresh_interval_is_one_second_only_while_playing() {
        let mut w = NowPlaying::new(false, false);
        let t0 = Instant::now();
        w.update(&ctx(t0, None));
        assert!(w.refresh_interval() > Duration::from_secs(60));
        w.update(&ctx(t0, Some(snap(true, 0, 60, t0))));
        assert_eq!(w.refresh_interval(), Duration::from_secs(1));
        w.update(&ctx(t0, Some(snap(false, 0, 60, t0))));
        assert!(w.refresh_interval() > Duration::from_secs(60));
    }

    #[test]
    fn hide_when_idle_hides_only_when_there_is_no_session() {
        let t0 = Instant::now();
        let mut w = NowPlaying::new(false, true);
        w.update(&ctx(t0, None));
        assert!(!w.visible(), "no session: hidden");
        w.update(&ctx(t0, Some(snap(false, 5, 100, t0))));
        assert!(w.visible(), "paused session: still shown");
        w.update(&ctx(t0, None));
        assert!(!w.visible(), "session gone: hidden again");
    }

    #[test]
    fn without_hide_when_idle_the_panel_is_always_visible() {
        let mut w = NowPlaying::new(false, false);
        w.update(&ctx(Instant::now(), None));
        assert!(w.visible());
    }

    #[test]
    fn declares_media_source() {
        assert_eq!(NowPlaying::new(false, false).sources(), &[Source::Media]);
    }
}
