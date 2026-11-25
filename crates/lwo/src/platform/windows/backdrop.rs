//! The edit-mode backdrop: a faint dark window behind the widgets that covers
//! every monitor. Clicking it, anywhere that isn't a widget, ends edit mode.
//!
//! This is an ordinary window of ours that receives ordinary clicks. No mouse
//! hook is installed, and it exists only while edit mode is on. The click that
//! ends edit mode is used up here: it does not also reach whatever is underneath.

use std::sync::atomic::{AtomicBool, Ordering};

use lwo_core::APP_NAME;
use windows::core::{Result, HSTRING, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{GetStockObject, BLACK_BRUSH, HBRUSH};
use windows::Win32::UI::WindowsAndMessaging::*;

/// Posted to the control window when the backdrop is clicked.
pub const WM_BACKDROP_CLICKED: u32 = WM_APP + 3;
/// Window opacity out of 255: visible as a dimming, but the screen stays readable.
const DIM_ALPHA: u8 = 60;

/// Set by a button press on the backdrop, so only a press *and* release there
/// counts. A widget drag that ends over the backdrop doesn't.
static PRESSED: AtomicBool = AtomicBool::new(false);

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN => {
            PRESSED.store(true, Ordering::Relaxed);
            LRESULT(0)
        }
        WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP => {
            if PRESSED.swap(false, Ordering::Relaxed) {
                // SAFETY: the user data holds the control window we stored at creation.
                // Posting (not calling) means this window can be destroyed safely.
                unsafe {
                    let control = HWND(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut _);
                    let _ = PostMessageW(Some(control), WM_BACKDROP_CLICKED, WPARAM(0), LPARAM(0));
                }
            }
            LRESULT(0)
        }
        // SAFETY: default handling for everything else.
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

pub struct Backdrop(HWND);

impl Backdrop {
    /// Cover all monitors and show. The caller then raises the widgets above it.
    pub fn show(hinst: HINSTANCE, control: HWND) -> Result<Self> {
        PRESSED.store(false, Ordering::Relaxed);
        let class = HSTRING::from(format!("{APP_NAME}-backdrop"));
        // SAFETY: registering a class (harmless if already registered) and creating
        // a popup window of it.
        unsafe {
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(proc),
                hInstance: hinst,
                lpszClassName: PCWSTR(class.as_ptr()),
                hbrBackground: HBRUSH(GetStockObject(BLACK_BRUSH).0),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                ..Default::default()
            });
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                &class,
                &HSTRING::from("Click to finish editing"),
                WS_POPUP,
                GetSystemMetrics(SM_XVIRTUALSCREEN),
                GetSystemMetrics(SM_YVIRTUALSCREEN),
                GetSystemMetrics(SM_CXVIRTUALSCREEN),
                GetSystemMetrics(SM_CYVIRTUALSCREEN),
                None,
                None,
                Some(hinst),
                None,
            )?;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, control.0 as isize);
            if let Err(e) = SetLayeredWindowAttributes(hwnd, COLORREF(0), DIM_ALPHA, LWA_ALPHA) {
                let _ = DestroyWindow(hwnd);
                return Err(e);
            }
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            Ok(Self(hwnd))
        }
    }
}

impl Drop for Backdrop {
    fn drop(&mut self) {
        // SAFETY: our window, not used afterwards.
        unsafe {
            let _ = DestroyWindow(self.0);
        }
    }
}
