//! The settings dialog: ordinary Win32 controls (check boxes, edit boxes,
//! drop-downs) built in code. No UI framework, so it costs nothing while it is
//! closed: the window and its controls only exist while it is open.
//!
//! The form's parsing and validation live in `lwo_core::form`; this file only
//! moves values between the controls and a `SettingsForm`.

use std::cell::RefCell;
use std::ffi::c_void;

use lwo_core::form::SettingsForm;
use lwo_core::theme::Theme;
use lwo_core::units::TemperatureUnit;
use lwo_core::APP_NAME;
use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateFontIndirectW, DeleteObject, GetSysColorBrush, COLOR_BTNFACE, HFONT, HGDIOBJ,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForSystem;
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::*;

use super::monitors;

const ID_OK: i32 = 1;
const ID_CANCEL: i32 = 2;
const ID_APPLY: i32 = 100;
const ID_EDIT_LAYOUT: i32 = 101;
const ID_RESET_LAYOUT: i32 = 102;

/// `BST_CHECKED` for check boxes.
const CHECKED: isize = 1;

struct Controls {
    clock_on: HWND,
    clock_refresh: HWND,
    clock_seconds: HWND,
    clock_24h: HWND,
    np_on: HWND,
    np_refresh: HWND,
    np_album: HWND,
    np_hide_idle: HWND,
    cpu_on: HWND,
    cpu_refresh: HWND,
    ram_on: HWND,
    ram_refresh: HWND,
    gpu_on: HWND,
    gpu_refresh: HWND,
    fps_on: HWND,
    fps_refresh: HWND,
    theme: HWND,
    units: HWND,
    scale: HWND,
    opacity: HWND,
    font: HWND,
    text_color: HWND,
    background_color: HWND,
    accent_color: HWND,
    hotkey_toggle: HWND,
    hotkey_edit: HWND,
}

struct Dialog {
    hwnd: HWND,
    font: HFONT,
    c: Controls,
}

thread_local! {
    static DIALOG: RefCell<Option<Dialog>> = const { RefCell::new(None) };
}

// ------------------------------------------------------- reading and writing controls

fn get_text(h: HWND) -> String {
    // SAFETY: plain window-text queries on a control we created.
    unsafe {
        let len = GetWindowTextLengthW(h).max(0) as usize;
        let mut buf = vec![0u16; len + 1];
        let n = GetWindowTextW(h, &mut buf).max(0) as usize;
        String::from_utf16_lossy(&buf[..n.min(len)])
    }
}

fn set_text(h: HWND, text: &str) {
    // SAFETY: plain window-text update on a control we created.
    unsafe {
        let _ = SetWindowTextW(h, &HSTRING::from(text));
    }
}

fn get_check(h: HWND) -> bool {
    // SAFETY: plain message to a button we created.
    unsafe { SendMessageW(h, BM_GETCHECK, Some(WPARAM(0)), Some(LPARAM(0))).0 == CHECKED }
}

fn set_check(h: HWND, on: bool) {
    // SAFETY: plain message to a button we created.
    unsafe {
        SendMessageW(
            h,
            BM_SETCHECK,
            Some(WPARAM(usize::from(on))),
            Some(LPARAM(0)),
        );
    }
}

fn get_choice(h: HWND) -> usize {
    // SAFETY: plain message to a combo box we created.
    unsafe {
        SendMessageW(h, CB_GETCURSEL, Some(WPARAM(0)), Some(LPARAM(0)))
            .0
            .max(0) as usize
    }
}

fn set_choice(h: HWND, index: usize) {
    // SAFETY: plain message to a combo box we created.
    unsafe {
        SendMessageW(h, CB_SETCURSEL, Some(WPARAM(index)), Some(LPARAM(0)));
    }
}

fn read_form(c: &Controls) -> SettingsForm {
    SettingsForm {
        clock_enabled: get_check(c.clock_on),
        clock_refresh: get_text(c.clock_refresh),
        clock_seconds: get_check(c.clock_seconds),
        clock_24h: get_check(c.clock_24h),
        now_playing_enabled: get_check(c.np_on),
        now_playing_refresh: get_text(c.np_refresh),
        now_playing_album: get_check(c.np_album),
        now_playing_hide_idle: get_check(c.np_hide_idle),
        cpu_enabled: get_check(c.cpu_on),
        cpu_refresh: get_text(c.cpu_refresh),
        ram_enabled: get_check(c.ram_on),
        ram_refresh: get_text(c.ram_refresh),
        gpu_enabled: get_check(c.gpu_on),
        gpu_refresh: get_text(c.gpu_refresh),
        fps_enabled: get_check(c.fps_on),
        fps_refresh: get_text(c.fps_refresh),
        theme: Theme::ALL[get_choice(c.theme).min(Theme::ALL.len() - 1)],
        temperature: TemperatureUnit::ALL[get_choice(c.units).min(TemperatureUnit::ALL.len() - 1)],
        scale_percent: get_text(c.scale),
        opacity_percent: get_text(c.opacity),
        font: get_text(c.font),
        text_color: get_text(c.text_color),
        background_color: get_text(c.background_color),
        accent_color: get_text(c.accent_color),
        hotkey_toggle: get_text(c.hotkey_toggle),
        hotkey_edit: get_text(c.hotkey_edit),
    }
}

fn write_form(c: &Controls, f: &SettingsForm) {
    set_check(c.clock_on, f.clock_enabled);
    set_text(c.clock_refresh, &f.clock_refresh);
    set_check(c.clock_seconds, f.clock_seconds);
    set_check(c.clock_24h, f.clock_24h);
    set_check(c.np_on, f.now_playing_enabled);
    set_text(c.np_refresh, &f.now_playing_refresh);
    set_check(c.np_album, f.now_playing_album);
    set_check(c.np_hide_idle, f.now_playing_hide_idle);
    set_check(c.cpu_on, f.cpu_enabled);
    set_text(c.cpu_refresh, &f.cpu_refresh);
    set_check(c.ram_on, f.ram_enabled);
    set_text(c.ram_refresh, &f.ram_refresh);
    set_check(c.gpu_on, f.gpu_enabled);
    set_text(c.gpu_refresh, &f.gpu_refresh);
    set_check(c.fps_on, f.fps_enabled);
    set_text(c.fps_refresh, &f.fps_refresh);
    set_choice(
        c.theme,
        Theme::ALL.iter().position(|&t| t == f.theme).unwrap_or(0),
    );
    set_choice(
        c.units,
        TemperatureUnit::ALL
            .iter()
            .position(|&t| t == f.temperature)
            .unwrap_or(0),
    );
    set_text(c.scale, &f.scale_percent);
    set_text(c.opacity, &f.opacity_percent);
    set_text(c.font, &f.font);
    set_text(c.text_color, &f.text_color);
    set_text(c.background_color, &f.background_color);
    set_text(c.accent_color, &f.accent_color);
    set_text(c.hotkey_toggle, &f.hotkey_toggle);
    set_text(c.hotkey_edit, &f.hotkey_edit);
}

// ----------------------------------------------------------------- building controls

/// Creates child controls at 96-DPI coordinates, scaled to the screen.
struct Builder {
    parent: HWND,
    hinst: HINSTANCE,
    scale: f32,
    font: HFONT,
}

impl Builder {
    fn px(&self, v: i32) -> i32 {
        (v as f32 * self.scale).round() as i32
    }

    #[allow(clippy::too_many_arguments)]
    fn make(
        &self,
        class: PCWSTR,
        text: &str,
        style: WINDOW_STYLE,
        ex: WINDOW_EX_STYLE,
        (x, y, w, h): (i32, i32, i32, i32),
        id: i32,
    ) -> HWND {
        // SAFETY: creating a child control of the dialog window.
        unsafe {
            let hwnd = CreateWindowExW(
                ex,
                class,
                &HSTRING::from(text),
                WS_CHILD | WS_VISIBLE | style,
                self.px(x),
                self.px(y),
                self.px(w),
                self.px(h),
                Some(self.parent),
                Some(HMENU(id as isize as *mut c_void)),
                Some(self.hinst),
                None,
            )
            .unwrap_or_default();
            SendMessageW(
                hwnd,
                WM_SETFONT,
                Some(WPARAM(self.font.0 as usize)),
                Some(LPARAM(1)),
            );
            hwnd
        }
    }

    fn label(&self, text: &str, x: i32, y: i32, w: i32) -> HWND {
        self.make(
            w!("STATIC"),
            text,
            WINDOW_STYLE(0),
            WINDOW_EX_STYLE(0),
            (x, y + 3, w, 18),
            0,
        )
    }

    fn group(&self, text: &str, x: i32, y: i32, w: i32, h: i32) -> HWND {
        self.make(
            w!("BUTTON"),
            text,
            WINDOW_STYLE(BS_GROUPBOX as u32),
            WINDOW_EX_STYLE(0),
            (x, y, w, h),
            0,
        )
    }

    fn check(&self, text: &str, x: i32, y: i32, w: i32) -> HWND {
        self.make(
            w!("BUTTON"),
            text,
            WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
            WINDOW_EX_STYLE(0),
            (x, y, w, 22),
            0,
        )
    }

    fn edit(&self, x: i32, y: i32, w: i32) -> HWND {
        self.make(
            w!("EDIT"),
            "",
            WS_TABSTOP | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
            WS_EX_CLIENTEDGE,
            (x, y, w, 22),
            0,
        )
    }

    fn combo(&self, x: i32, y: i32, w: i32, items: &[&str]) -> HWND {
        let hwnd = self.make(
            w!("COMBOBOX"),
            "",
            WS_TABSTOP | WS_VSCROLL | WINDOW_STYLE(CBS_DROPDOWNLIST as u32),
            WINDOW_EX_STYLE(0),
            (x, y, w, 120),
            0,
        );
        for item in items {
            let text = HSTRING::from(*item);
            // SAFETY: the string outlives the call; the combo box copies it.
            unsafe {
                SendMessageW(
                    hwnd,
                    CB_ADDSTRING,
                    Some(WPARAM(0)),
                    Some(LPARAM(text.as_ptr() as isize)),
                );
            }
        }
        hwnd
    }

    fn button(&self, text: &str, x: i32, y: i32, w: i32, id: i32, default: bool) -> HWND {
        let kind = if default {
            BS_DEFPUSHBUTTON
        } else {
            BS_PUSHBUTTON
        };
        self.make(
            w!("BUTTON"),
            text,
            WS_TABSTOP | WINDOW_STYLE(kind as u32),
            WINDOW_EX_STYLE(0),
            (x, y, w, 26),
            id,
        )
    }
}

/// The message font of the system (Segoe UI 9 pt on current Windows), so the
/// dialog looks native.
fn system_font() -> HFONT {
    let mut ncm = NONCLIENTMETRICSW {
        cbSize: size_of::<NONCLIENTMETRICSW>() as u32,
        ..Default::default()
    };
    // SAFETY: `ncm` is initialised with its size as the API requires.
    unsafe {
        let _ = SystemParametersInfoW(
            SPI_GETNONCLIENTMETRICS,
            ncm.cbSize,
            Some(&mut ncm as *mut NONCLIENTMETRICSW as *mut c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
        CreateFontIndirectW(&ncm.lfMessageFont)
    }
}

/// Shown under the FPS row, depending on whether this account may read frame events.
const FPS_NOTE_AVAILABLE: &str = "FPS shows the program in front, read from Windows' frame events. Off by default: how anti-cheat systems treat this is not known.";
const FPS_NOTE_UNAVAILABLE: &str = "FPS is unavailable: your account is not in the 'Performance Log Users' group (see the README). Joining lets any program you run trace system activity.";

/// Everything below the Widgets box is moved down by this much to fit the FPS row and note.
const DOWN: i32 = 62;

fn build(hwnd: HWND, hinst: HINSTANCE, scale: f32, font: HFONT, fps_available: bool) -> Controls {
    let b = Builder {
        parent: hwnd,
        hinst,
        scale,
        font,
    };

    // --- Widgets
    b.group("Widgets", 10, 6, 460, 196 + DOWN);
    b.label("Refresh (s)", 150, 24, 70);
    b.label("Options", 232, 24, 60);
    let row = |i: i32| 46 + i * 28;
    let clock_on = b.check("Clock", 24, row(0), 120);
    let clock_refresh = b.edit(160, row(0), 44);
    let clock_seconds = b.check("Show seconds", 232, row(0), 110);
    let clock_24h = b.check("24-hour", 350, row(0), 100);
    let np_on = b.check("Now Playing", 24, row(1), 120);
    let np_refresh = b.edit(160, row(1), 44);
    let np_album = b.check("Show album", 232, row(1), 110);
    let np_hide_idle = b.check("Hide when idle", 350, row(1), 110);
    let cpu_on = b.check("CPU", 24, row(2), 120);
    let cpu_refresh = b.edit(160, row(2), 44);
    let ram_on = b.check("Memory", 24, row(3), 120);
    let ram_refresh = b.edit(160, row(3), 44);
    let gpu_on = b.check("GPU", 24, row(4), 120);
    let gpu_refresh = b.edit(160, row(4), 44);
    let fps_on = b.check("FPS", 24, row(5), 120);
    let fps_refresh = b.edit(160, row(5), 44);
    b.make(
        w!("STATIC"),
        if fps_available {
            FPS_NOTE_AVAILABLE
        } else {
            FPS_NOTE_UNAVAILABLE
        },
        WINDOW_STYLE(0),
        WINDOW_EX_STYLE(0),
        (24, row(6) + 2, 436, 34),
        0,
    );
    if !fps_available {
        // The saved value is kept (reading a disabled box still works), it just can't be changed.
        // SAFETY: disabling controls we created.
        unsafe {
            let _ = EnableWindow(fps_on, false);
            let _ = EnableWindow(fps_refresh, false);
        }
    }

    // --- Appearance
    b.group("Appearance", 10, 208 + DOWN, 460, 170);
    b.label("Theme", 24, 230 + DOWN, 70);
    let theme_items: Vec<&str> = Theme::ALL.iter().map(|t| t.label()).collect();
    let theme = b.combo(100, 230 + DOWN, 130, &theme_items);
    b.label("Temperature", 250, 230 + DOWN, 80);
    let unit_items: Vec<&str> = TemperatureUnit::ALL.iter().map(|u| u.label()).collect();
    let units = b.combo(334, 230 + DOWN, 126, &unit_items);
    b.label("Scale (%)", 24, 260 + DOWN, 70);
    let scale_edit = b.edit(100, 260 + DOWN, 60);
    b.label("Opacity (%)", 250, 260 + DOWN, 80);
    let opacity = b.edit(334, 260 + DOWN, 60);
    b.label("Font", 24, 290 + DOWN, 70);
    let font_edit = b.edit(100, 290 + DOWN, 200);
    b.label(
        "Colours, as hex like #FFFFFF or #00000080. Leave blank to use the theme.",
        24,
        320 + DOWN,
        430,
    );
    b.label("Text", 24, 344 + DOWN, 34);
    let text_color = b.edit(60, 344 + DOWN, 80);
    b.label("Background", 150, 344 + DOWN, 74);
    let background_color = b.edit(228, 344 + DOWN, 80);
    b.label("Accent", 318, 344 + DOWN, 44);
    let accent_color = b.edit(366, 344 + DOWN, 80);

    // --- Hotkeys
    b.group("Hotkeys", 10, 384 + DOWN, 460, 62);
    b.label("Show / hide", 24, 408 + DOWN, 76);
    let hotkey_toggle = b.edit(104, 408 + DOWN, 130);
    b.label("Edit layout", 250, 408 + DOWN, 70);
    let hotkey_edit = b.edit(324, 408 + DOWN, 130);

    // --- Buttons
    b.button("Edit layout", 10, 458 + DOWN, 96, ID_EDIT_LAYOUT, false);
    b.button("Reset layout", 112, 458 + DOWN, 96, ID_RESET_LAYOUT, false);
    b.button("OK", 236, 458 + DOWN, 70, ID_OK, true);
    b.button("Cancel", 312, 458 + DOWN, 70, ID_CANCEL, false);
    b.button("Apply", 388, 458 + DOWN, 82, ID_APPLY, false);

    Controls {
        clock_on,
        clock_refresh,
        clock_seconds,
        clock_24h,
        np_on,
        np_refresh,
        np_album,
        np_hide_idle,
        cpu_on,
        cpu_refresh,
        ram_on,
        ram_refresh,
        gpu_on,
        gpu_refresh,
        fps_on,
        fps_refresh,
        theme,
        units,
        scale: scale_edit,
        opacity,
        font: font_edit,
        text_color,
        background_color,
        accent_color,
        hotkey_toggle,
        hotkey_edit,
    }
}

// --------------------------------------------------------------------- behaviour

fn message(owner: HWND, title: &str, text: &str, icon: MESSAGEBOX_STYLE) {
    // SAFETY: modal message box owned by the dialog.
    unsafe {
        MessageBoxW(
            Some(owner),
            &HSTRING::from(text),
            &HSTRING::from(title),
            MB_OK | icon,
        );
    }
}

/// Validate and apply the form. Returns true if the settings were applied.
fn apply(hwnd: HWND) -> bool {
    let Some(form) = DIALOG.with(|d| d.borrow().as_ref().map(|d| read_form(&d.c))) else {
        return false;
    };
    let Some(mut probe) = super::with_state(|s| s.config.clone()) else {
        return false;
    };
    // Check the numbers on a throwaway copy first, so a typo changes nothing.
    if let Err(errors) = form.apply_to(&mut probe) {
        message(
            hwnd,
            "Check your settings",
            &errors.join("\n"),
            MB_ICONWARNING,
        );
        return false;
    }
    let result = super::with_state(|s| {
        s.update_config(|cfg| {
            let _ = form.apply_to(cfg);
        })
    });
    match result {
        Some(Ok(warnings)) => {
            if !warnings.is_empty() {
                message(hwnd, APP_NAME, &warnings.join("\n"), MB_ICONINFORMATION);
            }
            refresh_from_config();
            true
        }
        Some(Err(problem)) => {
            message(hwnd, "Check your settings", &problem, MB_ICONWARNING);
            false
        }
        None => false,
    }
}

/// Show the values that are really in effect (for example clamped ones).
fn refresh_from_config() {
    if let Some(config) = super::with_state(|s| s.config.clone()) {
        let form = SettingsForm::from_config(&config);
        DIALOG.with(|d| {
            if let Some(d) = d.borrow().as_ref() {
                write_form(&d.c, &form);
            }
        });
    }
}

unsafe extern "system" fn dialog_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND => {
            match (wp.0 & 0xFFFF) as i32 {
                ID_OK => {
                    if apply(hwnd) {
                        // SAFETY: closing our own window.
                        unsafe {
                            let _ = DestroyWindow(hwnd);
                        }
                    }
                }
                ID_APPLY => {
                    apply(hwnd);
                }
                ID_CANCEL => {
                    // SAFETY: closing our own window.
                    unsafe {
                        let _ = DestroyWindow(hwnd);
                    }
                }
                ID_EDIT_LAYOUT => {
                    // Get out of the way, then let the user drag the widgets.
                    // SAFETY: closing our own window.
                    unsafe {
                        let _ = DestroyWindow(hwnd);
                    }
                    super::with_state(|s| s.enter_edit_mode());
                }
                ID_RESET_LAYOUT => {
                    let done = super::with_state(|s| s.update_config(|c| c.layout.clear()));
                    match done {
                        Some(Ok(_)) => message(
                            hwnd,
                            APP_NAME,
                            "Positions reset: the widgets are back in the default stack.",
                            MB_ICONINFORMATION,
                        ),
                        Some(Err(e)) => message(hwnd, APP_NAME, &e, MB_ICONWARNING),
                        None => {}
                    }
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            // SAFETY: closing our own window.
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            if let Some(d) = DIALOG.with(|d| d.borrow_mut().take()) {
                // SAFETY: the font was created for this dialog and nothing uses it any more.
                unsafe {
                    let _ = DeleteObject(HGDIOBJ(d.font.0));
                }
            }
            LRESULT(0)
        }
        // SAFETY: default handling for every other message.
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

/// Open the settings dialog, or bring it to the front if it is already open.
pub fn open(_control: HWND) {
    if let Some(existing) = DIALOG.with(|d| d.borrow().as_ref().map(|d| d.hwnd)) {
        // SAFETY: valid window handle.
        unsafe {
            let _ = SetForegroundWindow(existing);
        }
        return;
    }
    let Some((config, fps_available)) = super::with_state(|s| (s.config.clone(), s.info.fps))
    else {
        return;
    };

    // SAFETY: window class registration and creation on the UI thread.
    unsafe {
        let Ok(module) = GetModuleHandleW(None) else {
            return;
        };
        let hinst: HINSTANCE = module.into();
        let class = HSTRING::from(format!("{APP_NAME}-settings"));
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(dialog_proc),
            hInstance: hinst,
            lpszClassName: PCWSTR(class.as_ptr()),
            hbrBackground: GetSysColorBrush(COLOR_BTNFACE),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            ..Default::default()
        });

        let scale = GetDpiForSystem() as f32 / 96.0;
        let style = WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
        let ex = WS_EX_CONTROLPARENT | WS_EX_TOPMOST;
        let mut rc = RECT {
            left: 0,
            top: 0,
            right: (480.0 * scale).round() as i32,
            bottom: ((496 + DOWN) as f32 * scale).round() as i32,
        };
        let _ = AdjustWindowRectEx(&mut rc, style, false, ex);
        let (win_w, win_h) = (rc.right - rc.left, rc.bottom - rc.top);
        let screen = monitors::list()[0];
        let x = screen.left + (screen.width() - win_w) / 2;
        let y = screen.top + (screen.height() - win_h) / 2;

        let Ok(hwnd) = CreateWindowExW(
            ex,
            &class,
            &HSTRING::from(format!("{APP_NAME} settings")),
            style,
            x,
            y,
            win_w,
            win_h,
            None,
            None,
            Some(hinst),
            None,
        ) else {
            return;
        };

        let font = system_font();
        let controls = build(hwnd, hinst, scale, font, fps_available);
        write_form(&controls, &SettingsForm::from_config(&config));
        let first = controls.clock_on;
        DIALOG.with(|d| {
            *d.borrow_mut() = Some(Dialog {
                hwnd,
                font,
                c: controls,
            })
        });
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(Some(first));
    }
}

/// Let the dialog handle Tab, Enter and Esc. Returns true if it consumed the message.
pub fn is_dialog_message(msg: &MSG) -> bool {
    let Some(hwnd) = DIALOG.with(|d| d.borrow().as_ref().map(|d| d.hwnd)) else {
        return false;
    };
    // SAFETY: valid dialog window and message from the message loop.
    unsafe { IsDialogMessageW(hwnd, msg).as_bool() }
}

/// Keep an open settings window above other topmost windows (the edit-mode backdrop).
pub fn raise() {
    let Some(hwnd) = DIALOG.with(|d| d.try_borrow().ok().and_then(|d| d.as_ref().map(|d| d.hwnd)))
    else {
        return;
    };
    // SAFETY: valid window handle.
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}
