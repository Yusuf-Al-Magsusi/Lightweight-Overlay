//! Settings dialog (stub; replaced by the real dialog).
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::MSG;

pub fn open(_control: HWND) {}

pub fn is_dialog_message(_msg: &MSG) -> bool {
    false
}
