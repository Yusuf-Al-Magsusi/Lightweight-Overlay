//! When a widget wants its next update.
//!
//! A widget's wanted interval depends on its state (Now Playing asks for one
//! second while playing and an hour while paused). So the next update must be
//! planned from the state *after* the update has been applied. Planning from the
//! state before it is a bug: a track that starts playing would inherit the
//! hour-long wait of the paused state and its time would freeze.

use std::time::{Duration, Instant};

use crate::widget::{Context, Widget};

/// Per-widget update timing: the user's minimum interval plus when the next
/// update is due.
#[derive(Clone, Copy, Debug)]
pub struct Refresh {
    min_interval: Duration,
    next_due: Instant,
}

impl Refresh {
    /// Due immediately.
    pub fn new(min_interval: Duration, now: Instant) -> Self {
        Self {
            min_interval,
            next_due: now,
        }
    }

    pub fn next_due(&self) -> Instant {
        self.next_due
    }

    /// Whether the widget should be updated now. `tolerance` lets a tick that
    /// arrives slightly early still count, so the timer can line up with clock
    /// boundaries without skipping an update.
    pub fn is_due(&self, now: Instant, tolerance: Duration) -> bool {
        now + tolerance >= self.next_due
    }

    /// How long the widget wants between updates right now: what it asks for,
    /// but never faster than the user's refresh setting.
    pub fn interval(&self, widget: &dyn Widget) -> Duration {
        widget.refresh_interval().max(self.min_interval)
    }

    /// Update the widget, then plan the next update from its state afterwards.
    /// Returns whether what the widget draws changed.
    pub fn update(&mut self, widget: &mut dyn Widget, ctx: &Context) -> bool {
        let changed = widget.update(ctx);
        self.next_due = ctx.now + self.interval(widget);
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::now_playing::NowPlaying;
    use crate::widget::{LocalTime, MediaSnapshot};
    use std::sync::Arc;

    const SEC: Duration = Duration::from_secs(1);

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

    fn track(playing: bool, at: Instant) -> Arc<MediaSnapshot> {
        Arc::new(MediaSnapshot {
            title: "Song".into(),
            artist: "Artist".into(),
            album: String::new(),
            position: Duration::from_secs(30),
            duration: Duration::from_secs(200),
            playing,
            at,
        })
    }

    /// The bug that froze the time display: a widget that was idle or paused
    /// asked for an hour, and when playback started the next update was still
    /// planned an hour away.
    #[test]
    fn playback_starting_after_idle_is_planned_one_second_ahead_not_an_hour() {
        let t0 = Instant::now();
        let mut widget = NowPlaying::new(false, false);
        let mut refresh = Refresh::new(SEC, t0);

        refresh.update(&mut widget, &ctx(t0, None));
        assert!(
            refresh.next_due() >= t0 + Duration::from_secs(3000),
            "idle: the widget asks for a long wait"
        );

        let t1 = t0 + Duration::from_secs(10);
        refresh.update(&mut widget, &ctx(t1, Some(track(true, t1))));
        assert_eq!(
            refresh.next_due(),
            t1 + SEC,
            "playing: the very next update must be one second away"
        );
    }

    #[test]
    fn resuming_from_pause_is_planned_one_second_ahead() {
        let t0 = Instant::now();
        let mut widget = NowPlaying::new(false, false);
        let mut refresh = Refresh::new(SEC, t0);
        refresh.update(&mut widget, &ctx(t0, Some(track(false, t0))));
        assert!(refresh.next_due() > t0 + Duration::from_secs(3000));
        let t1 = t0 + Duration::from_secs(5);
        refresh.update(&mut widget, &ctx(t1, Some(track(true, t1))));
        assert_eq!(refresh.next_due(), t1 + SEC);
    }

    #[test]
    fn pausing_stops_the_one_second_updates() {
        let t0 = Instant::now();
        let mut widget = NowPlaying::new(false, false);
        let mut refresh = Refresh::new(SEC, t0);
        refresh.update(&mut widget, &ctx(t0, Some(track(true, t0))));
        assert_eq!(refresh.next_due(), t0 + SEC);
        let t1 = t0 + Duration::from_secs(4);
        refresh.update(&mut widget, &ctx(t1, Some(track(false, t1))));
        assert!(refresh.next_due() > t1 + Duration::from_secs(3000));
    }

    #[test]
    fn the_users_minimum_slows_a_widget_down_but_never_speeds_it_up() {
        let t0 = Instant::now();
        let mut widget = NowPlaying::new(false, false);

        // Playing wants 1 s, but the user chose 5 s.
        let mut slow = Refresh::new(Duration::from_secs(5), t0);
        slow.update(&mut widget, &ctx(t0, Some(track(true, t0))));
        assert_eq!(slow.next_due(), t0 + Duration::from_secs(5));

        // Paused wants an hour; a 1 s setting must not make it poll every second.
        let mut fast = Refresh::new(SEC, t0);
        fast.update(&mut widget, &ctx(t0, Some(track(false, t0))));
        assert!(fast.next_due() > t0 + Duration::from_secs(3000));
    }

    #[test]
    fn a_new_refresh_is_due_immediately_and_tolerance_counts_early_ticks() {
        let t0 = Instant::now();
        let r = Refresh::new(SEC, t0);
        assert!(r.is_due(t0, Duration::ZERO));

        let mut widget = NowPlaying::new(false, false);
        let mut r = Refresh::new(SEC, t0);
        r.update(&mut widget, &ctx(t0, Some(track(true, t0))));
        let early = t0 + Duration::from_millis(900);
        assert!(!r.is_due(early, Duration::ZERO));
        assert!(r.is_due(early, Duration::from_millis(120)));
    }
}
