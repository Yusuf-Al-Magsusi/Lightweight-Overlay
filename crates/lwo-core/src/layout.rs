//! Placement maths, independent of the OS: anchoring a widget to a monitor
//! corner, turning a dragged window position back into a saved placement, and
//! stacking widgets by default. All coordinates are pixels.
//!
//! Saved placements are stored in 100 %-DPI units, so they survive a change of
//! display scaling, and relative to the nearest monitor corner, so they survive
//! a change of resolution.

use serde::{Deserialize, Serialize};

use crate::config::Anchor;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
}

/// Where one widget sits. Saved in the config under `[layout.<widget id>]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Placement {
    /// 0 is the primary monitor; 1 and up are the others in system order.
    pub monitor: u32,
    pub anchor: Anchor,
    /// Distance from the anchored corner, at 100 % DPI.
    pub x: i32,
    pub y: i32,
}

impl Default for Placement {
    fn default() -> Self {
        Self {
            monitor: 0,
            anchor: Anchor::TopLeft,
            x: 20,
            y: 20,
        }
    }
}

/// Top-left position of a `w` by `h` window anchored to `monitor` with the given offsets.
pub fn anchored(
    anchor: Anchor,
    off_x: i32,
    off_y: i32,
    w: i32,
    h: i32,
    monitor: Rect,
) -> (i32, i32) {
    match anchor {
        Anchor::TopLeft => (monitor.left + off_x, monitor.top + off_y),
        Anchor::TopRight => (monitor.right - w - off_x, monitor.top + off_y),
        Anchor::BottomLeft => (monitor.left + off_x, monitor.bottom - h - off_y),
        Anchor::BottomRight => (monitor.right - w - off_x, monitor.bottom - h - off_y),
    }
}

/// The monitor corner closest to the window's centre.
pub fn nearest_anchor(monitor: Rect, window: Rect) -> Anchor {
    let cx = window.left + window.width() / 2;
    let cy = window.top + window.height() / 2;
    let right = cx > monitor.left + monitor.width() / 2;
    let bottom = cy > monitor.top + monitor.height() / 2;
    match (right, bottom) {
        (false, false) => Anchor::TopLeft,
        (true, false) => Anchor::TopRight,
        (false, true) => Anchor::BottomLeft,
        (true, true) => Anchor::BottomRight,
    }
}

/// A saved placement for a window that now occupies `window` on `monitor`.
pub fn placement_from_rect(
    monitor_index: u32,
    monitor: Rect,
    window: Rect,
    dpi_scale: f32,
) -> Placement {
    let anchor = nearest_anchor(monitor, window);
    let (dx, dy) = match anchor {
        Anchor::TopLeft => (window.left - monitor.left, window.top - monitor.top),
        Anchor::TopRight => (monitor.right - window.right, window.top - monitor.top),
        Anchor::BottomLeft => (window.left - monitor.left, monitor.bottom - window.bottom),
        Anchor::BottomRight => (monitor.right - window.right, monitor.bottom - window.bottom),
    };
    let unscale = |v: i32| (v as f32 / dpi_scale).round() as i32;
    Placement {
        monitor: monitor_index,
        anchor,
        x: unscale(dx),
        y: unscale(dy),
    }
}

/// Window position for a saved placement on `monitor`, kept fully on screen
/// (if the window fits) so a changed resolution never strands a widget.
pub fn position_from_placement(
    p: &Placement,
    monitor: Rect,
    w: i32,
    h: i32,
    dpi_scale: f32,
) -> (i32, i32) {
    let scale = |v: i32| (v as f32 * dpi_scale).round() as i32;
    let (x, y) = anchored(p.anchor, scale(p.x), scale(p.y), w, h, monitor);
    clamp_to_monitor(x, y, w, h, monitor)
}

/// Move a `w` by `h` window so it lies inside `monitor`, when it is small enough to fit.
pub fn clamp_to_monitor(x: i32, y: i32, w: i32, h: i32, monitor: Rect) -> (i32, i32) {
    let max_x = (monitor.right - w).max(monitor.left);
    let max_y = (monitor.bottom - h).max(monitor.top);
    (x.clamp(monitor.left, max_x), y.clamp(monitor.top, max_y))
}

/// Default arrangement: widgets stacked top to bottom with `gap` between, left
/// aligned. Returns each widget's offset inside the group box, and the box size.
pub fn stack(sizes: &[(i32, i32)], gap: i32) -> (Vec<(i32, i32)>, (i32, i32)) {
    let mut y = 0;
    let mut offsets = Vec::with_capacity(sizes.len());
    for (i, (_, h)) in sizes.iter().enumerate() {
        if i > 0 {
            y += gap;
        }
        offsets.push((0, y));
        y += h;
    }
    let width = sizes.iter().map(|s| s.0).max().unwrap_or(0);
    (offsets, (width, y))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MON: Rect = Rect {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1080,
    };

    #[test]
    fn anchors_to_each_corner() {
        assert_eq!(anchored(Anchor::TopLeft, 20, 30, 100, 50, MON), (20, 30));
        assert_eq!(anchored(Anchor::TopRight, 20, 30, 100, 50, MON), (1800, 30));
        assert_eq!(
            anchored(Anchor::BottomLeft, 20, 30, 100, 50, MON),
            (20, 1000)
        );
        assert_eq!(
            anchored(Anchor::BottomRight, 20, 30, 100, 50, MON),
            (1800, 1000)
        );
    }

    #[test]
    fn works_on_a_monitor_that_does_not_start_at_the_origin() {
        let second = Rect::new(1920, -200, 3840, 880);
        assert_eq!(
            anchored(Anchor::TopLeft, 10, 10, 50, 50, second),
            (1930, -190)
        );
        // right edge 3840 - width 50 - offset 10 = 3780; bottom edge 880 - height 50 - offset 10 = 820
        assert_eq!(
            anchored(Anchor::BottomRight, 10, 10, 50, 50, second),
            (3780, 820)
        );
    }

    #[test]
    fn nearest_anchor_follows_the_quadrant_of_the_window_centre() {
        let win = |l, t| Rect::new(l, t, l + 100, t + 50);
        assert_eq!(nearest_anchor(MON, win(10, 10)), Anchor::TopLeft);
        assert_eq!(nearest_anchor(MON, win(1800, 10)), Anchor::TopRight);
        assert_eq!(nearest_anchor(MON, win(10, 1000)), Anchor::BottomLeft);
        assert_eq!(nearest_anchor(MON, win(1800, 1000)), Anchor::BottomRight);
    }

    #[test]
    fn dragged_position_round_trips_through_a_saved_placement() {
        for &dpi in &[1.0_f32, 1.25, 1.5, 2.0] {
            for &(l, t) in &[(15, 25), (1790, 40), (30, 990), (1700, 950)] {
                let win = Rect::new(l, t, l + 120, t + 60);
                let p = placement_from_rect(0, MON, win, dpi);
                let (x, y) = position_from_placement(&p, MON, 120, 60, dpi);
                assert!(
                    (x - l).abs() <= 1 && (y - t).abs() <= 1,
                    "dpi {dpi}: ({l},{t}) came back as ({x},{y})"
                );
            }
        }
    }

    #[test]
    fn placements_survive_a_resolution_change_by_staying_near_their_corner() {
        // Saved on a 1920x1080 screen at the bottom-right...
        let win = Rect::new(1700, 950, 1820, 1010);
        let p = placement_from_rect(0, MON, win, 1.0);
        assert_eq!(p.anchor, Anchor::BottomRight);
        // ...then the monitor becomes 2560x1440: still 100 px from the right edge, 70 from the bottom.
        let big = Rect::new(0, 0, 2560, 1440);
        assert_eq!(
            position_from_placement(&p, big, 120, 60, 1.0),
            // right edge 2560 - 100 (distance) - 120 (width); bottom 1440 - 70 - 60
            (2560 - 100 - 120, 1440 - 70 - 60)
        );
    }

    #[test]
    fn a_placement_outside_the_monitor_is_pulled_back_on_screen() {
        let p = Placement {
            monitor: 0,
            anchor: Anchor::TopLeft,
            x: 5000,
            y: -300,
        };
        assert_eq!(position_from_placement(&p, MON, 200, 100, 1.0), (1720, 0));
    }

    #[test]
    fn a_window_larger_than_the_monitor_does_not_panic_or_flip() {
        assert_eq!(clamp_to_monitor(50, 50, 4000, 4000, MON), (0, 0));
    }

    #[test]
    fn stacks_widgets_with_a_gap_and_left_aligns_them() {
        let (offsets, (w, h)) = stack(&[(150, 70), (300, 110), (200, 60)], 6);
        assert_eq!(offsets, vec![(0, 0), (0, 76), (0, 192)]);
        assert_eq!((w, h), (300, 252));
    }

    #[test]
    fn stacking_nothing_is_empty() {
        assert_eq!(stack(&[], 6), (vec![], (0, 0)));
    }

    #[test]
    fn default_placement_is_the_old_top_left_20_20() {
        let p = Placement::default();
        assert_eq!(
            (p.monitor, p.anchor, p.x, p.y),
            (0, Anchor::TopLeft, 20, 20)
        );
    }
}
