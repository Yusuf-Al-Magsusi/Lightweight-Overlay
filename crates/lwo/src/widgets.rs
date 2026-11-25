//! Builds the widgets that the config switches on, in their default stacking order.

use lwo_core::clock::Clock;
use lwo_core::config::Config;
use lwo_core::fps::FpsWidget;
use lwo_core::now_playing::NowPlaying;
use lwo_core::stats::{CpuWidget, GpuWidget, RamWidget};
use lwo_core::widget::Widget;

pub fn build(config: &Config) -> Vec<Box<dyn Widget>> {
    let w = &config.widgets;
    let mut out: Vec<Box<dyn Widget>> = Vec::new();
    if w.clock.enabled {
        out.push(Box::new(Clock::new(w.clock.show_seconds, w.clock.use_24h)));
    }
    if w.now_playing.enabled {
        out.push(Box::new(NowPlaying::new(
            w.now_playing.show_album,
            w.now_playing.hide_when_idle,
        )));
    }
    if w.cpu.enabled {
        out.push(Box::new(CpuWidget::new(w.cpu.show_power)));
    }
    if w.ram.enabled {
        out.push(Box::new(RamWidget::new()));
    }
    if w.gpu.enabled {
        out.push(Box::new(GpuWidget::new()));
    }
    if w.fps.enabled {
        out.push(Box::new(FpsWidget::new()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use lwo_core::config::WIDGET_IDS;

    #[test]
    fn every_widget_is_built_in_the_documented_order() {
        let mut c = Config::default();
        c.widgets.fps.enabled = true;
        let ids: Vec<_> = build(&c).iter().map(|w| w.id()).collect();
        assert_eq!(ids, WIDGET_IDS);
    }

    #[test]
    fn fps_is_not_built_unless_switched_on() {
        assert!(build(&Config::default()).iter().all(|w| w.id() != "fps"));
    }

    #[test]
    fn disabled_widgets_are_not_built() {
        let mut c = Config::default();
        c.widgets.clock.enabled = false;
        c.widgets.gpu.enabled = false;
        let ids: Vec<_> = build(&c).iter().map(|w| w.id()).collect();
        assert_eq!(ids, ["now_playing", "cpu", "ram"]);
    }

    #[test]
    fn nothing_enabled_builds_nothing() {
        let mut c = Config::default();
        c.widgets.clock.enabled = false;
        c.widgets.now_playing.enabled = false;
        c.widgets.cpu.enabled = false;
        c.widgets.ram.enabled = false;
        c.widgets.gpu.enabled = false;
        assert!(build(&c).is_empty());
    }
}
