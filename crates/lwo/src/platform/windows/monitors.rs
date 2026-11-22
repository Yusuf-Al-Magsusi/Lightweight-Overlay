//! Monitor enumeration. Index 0 is always the primary monitor; the others
//! follow in the order Windows reports them. Saved layouts refer to monitors
//! by this index.

use lwo_core::layout::Rect;
use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, MonitorFromWindow, HDC, HMONITOR, MONITORINFO,
    MONITOR_DEFAULTTONEAREST,
};

const MONITORINFOF_PRIMARY: u32 = 1;

fn rect_of(r: RECT) -> Rect {
    Rect::new(r.left, r.top, r.right, r.bottom)
}

fn info(mon: HMONITOR) -> Option<MONITORINFO> {
    let mut mi = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: `mi` is initialised with its size as the API requires.
    unsafe { GetMonitorInfoW(mon, &mut mi).as_bool().then_some(mi) }
}

unsafe extern "system" fn collect(mon: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
    // SAFETY: `data` is the `Vec` passed by `list`, alive for the whole enumeration.
    let list = unsafe { &mut *(data.0 as *mut Vec<(Rect, bool)>) };
    if let Some(mi) = info(mon) {
        list.push((
            rect_of(mi.rcMonitor),
            mi.dwFlags & MONITORINFOF_PRIMARY != 0,
        ));
    }
    BOOL(1)
}

/// Every monitor's full rectangle, primary first.
pub fn list() -> Vec<Rect> {
    let mut found: Vec<(Rect, bool)> = Vec::new();
    // SAFETY: the callback only touches `found`, which outlives the call.
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(collect),
            LPARAM(&mut found as *mut Vec<(Rect, bool)> as isize),
        );
    }
    // `false < true`, so sort descending on "is primary"; the sort is stable.
    found.sort_by_key(|&(_, primary)| std::cmp::Reverse(primary));
    let mut rects: Vec<Rect> = found.into_iter().map(|(r, _)| r).collect();
    if rects.is_empty() {
        rects.push(Rect::new(0, 0, 1920, 1080)); // should never happen; keep layout maths safe
    }
    rects
}

/// The monitor a window is mostly on: its index in `list()` and its rectangle.
pub fn of_window(hwnd: HWND) -> (u32, Rect) {
    let rects = list();
    // SAFETY: plain query.
    let mon = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    let rect = info(mon).map(|mi| rect_of(mi.rcMonitor));
    let index = rect
        .and_then(|r| rects.iter().position(|&x| x == r))
        .unwrap_or(0);
    (index as u32, rects[index])
}
