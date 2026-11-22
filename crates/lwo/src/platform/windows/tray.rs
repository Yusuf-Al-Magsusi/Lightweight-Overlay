//! Notification-area (tray) icon and its right-click menu.

use lwo_core::APP_NAME;
use windows::core::{w, Result};
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

/// Message the shell sends to the control window for tray mouse events.
pub const WM_TRAY: u32 = WM_APP + 1;
pub const ID_TOGGLE: usize = 1;
pub const ID_QUIT: usize = 2;
const ICON_ID: u32 = 1;

fn copy_wide(dst: &mut [u16], s: &str) {
    let max = dst.len() - 1;
    for (slot, unit) in dst.iter_mut().zip(s.encode_utf16().take(max)) {
        *slot = unit;
    }
}

fn base_data(hwnd: HWND) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: ICON_ID,
        ..Default::default()
    }
}

/// Add (or re-add, after Explorer restarts) the tray icon.
pub fn add(hwnd: HWND) -> Result<()> {
    let mut nid = base_data(hwnd);
    nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    nid.uCallbackMessage = WM_TRAY;
    // SAFETY: loading a stock icon; `nid` is fully initialised.
    unsafe {
        nid.hIcon = LoadIconW(None, IDI_APPLICATION)?;
        copy_wide(&mut nid.szTip, APP_NAME);
        Shell_NotifyIconW(NIM_ADD, &nid).ok()
    }
}

pub fn remove(hwnd: HWND) {
    let nid = base_data(hwnd);
    // SAFETY: `nid` identifies the icon added in `add`.
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
    }
}

/// One-time balloon for problems such as an unreadable config.
pub fn warn(hwnd: HWND, message: &str) {
    let mut nid = base_data(hwnd);
    nid.uFlags = NIF_INFO;
    nid.dwInfoFlags = NIIF_WARNING;
    copy_wide(&mut nid.szInfoTitle, APP_NAME);
    copy_wide(&mut nid.szInfo, message);
    // SAFETY: modifies the icon added in `add`.
    unsafe {
        let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
    }
}

/// Show the context menu at the cursor.
pub fn show_menu(hwnd: HWND, toggle_label: &str) {
    // SAFETY: standard popup-menu sequence; the menu is destroyed before returning.
    unsafe {
        let Ok(menu) = CreatePopupMenu() else {
            return;
        };
        let label = windows::core::HSTRING::from(toggle_label);
        let _ = AppendMenuW(menu, MF_STRING, ID_TOGGLE, &label);
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        let _ = AppendMenuW(menu, MF_STRING, ID_QUIT, w!("Quit"));
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        // Required so the menu closes when the user clicks elsewhere.
        let _ = SetForegroundWindow(hwnd);
        let _ = TrackPopupMenu(
            menu,
            TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
            pt.x,
            pt.y,
            None,
            hwnd,
            None,
        );
        let _ = PostMessageW(Some(hwnd), WM_NULL, Default::default(), Default::default());
        let _ = DestroyMenu(menu);
    }
}
