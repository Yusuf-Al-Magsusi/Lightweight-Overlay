//! Windows platform: a click-through layered window plus a hidden control
//! window that owns the hotkey, timer and tray icon.
//!
//! Lazy by design: while the overlay is hidden the timer is stopped, and while
//! it is visible the window is only redrawn when a widget reports a change.
//! Between events the thread blocks in `GetMessageW`.

mod canvas;
mod tray;

use std::cell::RefCell;
use std::sync::atomic::{AtomicU32, Ordering};

use lwo_core::config::Anchor;
use lwo_core::hotkey::{Hotkey, Key};
use lwo_core::widget::{Color, Context, LocalTime, Style, Widget};
use lwo_core::APP_NAME;
use windows::core::{w, Result, HSTRING};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use self::canvas::{Measurer, Surface};
use super::App;
use lwo_core::widget::TextMeasure;

const TIMER_ID: usize = 1;
const HOTKEY_ID: i32 = 1;
const WIDGET_GAP: f32 = 6.0;

static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

struct State {
    control: HWND,
    overlay: HWND,
    surface: Surface,
    widgets: Vec<Box<dyn Widget>>,
    style: Style,
    /// Widget heights, in stacking order.
    heights: Vec<f32>,
    gap: f32,
    pos: POINT,
    opacity: u8,
    visible: bool,
    toggle_label: String,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

/// Run `f` on the state. Returns `None` if it is not set up yet or already
/// borrowed (a message arriving while we are inside a call, which we ignore).
fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
    STATE.with(|cell| {
        let mut guard = cell.try_borrow_mut().ok()?;
        guard.as_mut().map(f)
    })
}

fn local_time() -> (LocalTime, u16) {
    // SAFETY: GetLocalTime has no preconditions.
    let st = unsafe { GetLocalTime() };
    (
        LocalTime {
            hour: st.wHour as u8,
            minute: st.wMinute as u8,
            second: st.wSecond as u8,
        },
        st.wMilliseconds,
    )
}

impl State {
    /// Redraw if anything changed (or `force`). Returns whether it drew.
    fn draw(&mut self, force: bool) -> Result<bool> {
        let (time, _) = local_time();
        let ctx = Context { time };
        let mut changed = force;
        for w in &mut self.widgets {
            changed |= w.update(&ctx);
        }
        if !changed {
            return Ok(false);
        }

        let widgets = &self.widgets;
        let heights = &self.heights;
        let style = &self.style;
        let gap = self.gap;
        self.surface.paint(|frame| {
            let mut y = 0.0;
            for (w, h) in widgets.iter().zip(heights) {
                frame.set_origin(0.0, y);
                w.render(frame, style);
                y += h + gap;
            }
        })?;

        let size = SIZE {
            cx: self.surface.width,
            cy: self.surface.height,
        };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: self.opacity,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        // SAFETY: valid window and DC; all pointers outlive the call.
        unsafe {
            let screen = GetDC(None);
            let r = UpdateLayeredWindow(
                self.overlay,
                Some(screen),
                Some(&self.pos),
                Some(&size),
                Some(self.surface.mem_dc),
                Some(&POINT { x: 0, y: 0 }),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );
            ReleaseDC(None, screen);
            r?;
        }
        Ok(true)
    }

    /// Milliseconds until the next update is due, lined up with the second
    /// boundary for 1 s widgets so a clock never shows a stale second.
    fn next_delay_ms(&self) -> u32 {
        let min = self
            .widgets
            .iter()
            .map(|w| w.refresh_interval().as_millis() as u32)
            .min()
            .unwrap_or(1000)
            .max(50);
        if min == 1000 {
            let (_, ms) = local_time();
            (1000 - ms as u32 + 5).clamp(20, 1005)
        } else {
            min
        }
    }

    fn schedule(&self) {
        // SAFETY: valid control window; replaces any existing timer with the same id.
        unsafe {
            SetTimer(Some(self.control), TIMER_ID, self.next_delay_ms(), None);
        }
    }

    fn show(&mut self) {
        if self.widgets.is_empty() {
            return;
        }
        let _ = self.draw(true);
        // SAFETY: valid overlay window. Re-asserting TOPMOST keeps us above
        // borderless-fullscreen windows that took the top spot meanwhile.
        unsafe {
            let _ = ShowWindow(self.overlay, SW_SHOWNOACTIVATE);
            let _ = SetWindowPos(
                self.overlay,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
        self.visible = true;
        self.schedule();
    }

    fn hide(&mut self) {
        // SAFETY: valid windows; stopping the timer means a hidden overlay does no work.
        unsafe {
            let _ = KillTimer(Some(self.control), TIMER_ID);
            let _ = ShowWindow(self.overlay, SW_HIDE);
        }
        self.visible = false;
    }

    fn toggle(&mut self) {
        if self.visible {
            self.hide();
        } else {
            self.show();
        }
    }

    fn tick(&mut self) {
        if self.visible {
            let _ = self.draw(false);
            self.schedule();
        }
    }
}

fn vk_of(key: Key) -> u32 {
    match key {
        Key::Char(c) => c as u32, // VK codes for A-Z and 0-9 equal their ASCII values
        Key::F(n) => 0x70 + (n as u32 - 1),
        Key::Space => 0x20,
        Key::PageUp => 0x21,
        Key::PageDown => 0x22,
        Key::End => 0x23,
        Key::Home => 0x24,
        Key::Insert => 0x2D,
        Key::Delete => 0x2E,
    }
}

fn register_hotkey(hwnd: HWND, hk: Hotkey) -> Result<()> {
    let mut mods = MOD_NOREPEAT;
    if hk.ctrl {
        mods |= MOD_CONTROL;
    }
    if hk.alt {
        mods |= MOD_ALT;
    }
    if hk.shift {
        mods |= MOD_SHIFT;
    }
    if hk.win {
        mods |= MOD_WIN;
    }
    // SAFETY: valid window handle owned by this thread.
    unsafe { RegisterHotKey(Some(hwnd), HOTKEY_ID, mods, vk_of(hk.key)) }
}

unsafe extern "system" fn control_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let taskbar_created = TASKBAR_CREATED.load(Ordering::Relaxed);
    match msg {
        WM_TIMER if wp.0 == TIMER_ID => {
            with_state(|s| s.tick());
            LRESULT(0)
        }
        WM_HOTKEY if wp.0 as i32 == HOTKEY_ID => {
            with_state(|s| s.toggle());
            LRESULT(0)
        }
        tray::WM_TRAY => {
            match (lp.0 & 0xFFFF) as u32 {
                WM_LBUTTONUP => {
                    with_state(|s| s.toggle());
                }
                WM_RBUTTONUP => {
                    let label = with_state(|s| s.toggle_label.clone()).unwrap_or_default();
                    tray::show_menu(hwnd, &label);
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            match wp.0 & 0xFFFF {
                tray::ID_TOGGLE => {
                    with_state(|s| s.toggle());
                }
                tray::ID_QUIT => {
                    // SAFETY: destroying our own window.
                    unsafe {
                        let _ = DestroyWindow(hwnd);
                    }
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            tray::remove(hwnd);
            // SAFETY: ends this thread's message loop.
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        m if m == taskbar_created && m != 0 => {
            // Explorer restarted; its tray is empty again.
            let _ = tray::add(hwnd);
            LRESULT(0)
        }
        // SAFETY: default handling for every other message.
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

unsafe extern "system" fn overlay_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    // SAFETY: default handling; the overlay never handles its own messages.
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

fn anchored_position(anchor: Anchor, off_x: i32, off_y: i32, w: i32, h: i32) -> POINT {
    // SAFETY: querying the primary monitor.
    let rc = unsafe {
        let mon = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let _ = GetMonitorInfoW(mon, &mut info);
        info.rcMonitor
    };
    let (x, y) = match anchor {
        Anchor::TopLeft => (rc.left + off_x, rc.top + off_y),
        Anchor::TopRight => (rc.right - w - off_x, rc.top + off_y),
        Anchor::BottomLeft => (rc.left + off_x, rc.bottom - h - off_y),
        Anchor::BottomRight => (rc.right - w - off_x, rc.bottom - h - off_y),
    };
    POINT { x, y }
}

pub fn run(app: App) -> std::result::Result<(), String> {
    run_inner(app).map_err(|e| e.to_string())
}

fn run_inner(app: App) -> Result<()> {
    let App {
        config,
        widgets,
        mut warnings,
        ..
    } = app;

    // SAFETY: process-wide setup, done once before any window exists.
    unsafe {
        // Without this Windows bitmap-scales the overlay on high-DPI screens.
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        // One instance at a time.
        let name = HSTRING::from(format!("Local\\{APP_NAME}-single-instance"));
        // The handle is intentionally never closed: it must live until the process exits.
        let _mutex = CreateMutexW(None, false, &name)?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return Ok(());
        }
    }

    let dpi_scale = unsafe { GetDpiForSystem() } as f32 / 96.0;
    let o = &config.overlay;
    let style = Style {
        font: o.font.clone(),
        scale: o.scale * dpi_scale,
        text: Color::new(1.0, 1.0, 1.0, 1.0),
        background: Color::new(0.05, 0.05, 0.08, 0.7),
    };

    // Size the overlay from the widgets' own measurements.
    let measurer = Measurer::new()?;
    let sizes: Vec<(f32, f32)> = widgets
        .iter()
        .map(|w| w.size(&measurer as &dyn TextMeasure, &style))
        .collect();
    let gap = WIDGET_GAP * style.scale;
    let width = sizes.iter().map(|s| s.0).fold(0.0_f32, f32::max).ceil() as i32;
    let height = (sizes.iter().map(|s| s.1).sum::<f32>()
        + gap * sizes.len().saturating_sub(1) as f32)
        .ceil() as i32;
    let (win_w, win_h) = (width.max(1), height.max(1));

    let pos = anchored_position(
        o.anchor,
        (o.offset_x as f32 * dpi_scale) as i32,
        (o.offset_y as f32 * dpi_scale) as i32,
        win_w,
        win_h,
    );

    // SAFETY: window class registration and creation on this thread.
    let (control, overlay) = unsafe {
        let hinst: HINSTANCE = GetModuleHandleW(None)?.into();
        let overlay_class = HSTRING::from(format!("{APP_NAME}-overlay"));
        let control_class = HSTRING::from(format!("{APP_NAME}-control"));
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(overlay_proc),
            hInstance: hinst,
            lpszClassName: windows::core::PCWSTR(overlay_class.as_ptr()),
            ..Default::default()
        });
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(control_proc),
            hInstance: hinst,
            lpszClassName: windows::core::PCWSTR(control_class.as_ptr()),
            ..Default::default()
        });
        TASKBAR_CREATED.store(
            RegisterWindowMessageW(w!("TaskbarCreated")),
            Ordering::Relaxed,
        );

        let overlay = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            &overlay_class,
            &HSTRING::from(APP_NAME),
            WS_POPUP,
            pos.x,
            pos.y,
            win_w,
            win_h,
            None,
            None,
            Some(hinst),
            None,
        )?;
        // Never shown; exists to receive the hotkey, timer and tray messages.
        let control = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            &control_class,
            &HSTRING::from(APP_NAME),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(hinst),
            None,
        )?;
        (control, overlay)
    };

    let toggle = match Hotkey::parse(&config.hotkeys.toggle) {
        Ok(hk) => match register_hotkey(control, hk) {
            Ok(()) => Some(hk),
            Err(e) => {
                warnings.push(format!(
                    "Hotkey {hk} is unavailable ({e}); another program may be using it. Use the tray icon instead."
                ));
                None
            }
        },
        Err(e) => {
            warnings.push(format!(
                "Invalid toggle hotkey: {e}. Use the tray icon instead."
            ));
            None
        }
    };
    let toggle_label = match toggle {
        Some(hk) => format!("Show / hide overlay ({hk})"),
        None => "Show / hide overlay".to_string(),
    };

    let start_visible = config.overlay.start_visible;
    let state = State {
        control,
        overlay,
        surface: Surface::new(win_w, win_h)?,
        heights: sizes.iter().map(|s| s.1).collect(),
        widgets,
        style,
        gap,
        pos,
        opacity: (config.overlay.opacity * 255.0).round() as u8,
        visible: false,
        toggle_label,
    };
    STATE.with(|cell| *cell.borrow_mut() = Some(state));

    tray::add(control)?;
    if !warnings.is_empty() {
        tray::warn(control, &warnings.join("\n"));
    }
    if start_visible {
        with_state(|s| s.show());
    }

    // Blocks here; no CPU use until the timer, hotkey or tray posts a message.
    let mut msg = MSG::default();
    // SAFETY: standard message loop on the thread that created the windows.
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}
