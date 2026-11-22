//! One widget in its own layered window.
//!
//! Normally the window is click-through. In edit mode the click-through flag is
//! removed and the window answers `WM_NCHITTEST` with "title bar", so Windows'
//! own window dragging moves it (see `overlay_proc`).

use std::time::{Duration, Instant};

use lwo_core::layout::Rect;
use lwo_core::schedule::Refresh;
use lwo_core::widget::{Color, Context, Style, Widget};
use windows::core::{Result, HSTRING};
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, POINT, RECT, SIZE};
use windows::Win32::Graphics::Gdi::{GetDC, ReleaseDC, AC_SRC_ALPHA, AC_SRC_OVER, BLENDFUNCTION};
use windows::Win32::UI::WindowsAndMessaging::*;

use super::canvas::{Gfx, Surface};

pub struct Panel {
    pub id: &'static str,
    pub widget: Box<dyn Widget>,
    pub hwnd: HWND,
    surface: Surface,
    /// When this widget is next due for an update.
    refresh: Refresh,
    shown: bool,
}

impl Panel {
    pub fn create(
        widget: Box<dyn Widget>,
        size: (i32, i32),
        pos: (i32, i32),
        min_interval: Duration,
        gfx: &Gfx,
        hinst: HINSTANCE,
        class: &HSTRING,
    ) -> Result<Self> {
        let surface = Surface::new(gfx, size.0.max(1), size.1.max(1))?;
        // SAFETY: creating a top-level popup window of our registered class.
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED
                    | WS_EX_TRANSPARENT
                    | WS_EX_TOPMOST
                    | WS_EX_TOOLWINDOW
                    | WS_EX_NOACTIVATE,
                class,
                &HSTRING::from(widget.display_name()),
                WS_POPUP,
                pos.0,
                pos.1,
                size.0.max(1),
                size.1.max(1),
                None,
                None,
                Some(hinst),
                None,
            )?
        };
        Ok(Self {
            id: widget.id(),
            widget,
            hwnd,
            surface,
            refresh: Refresh::new(min_interval, Instant::now()),
            shown: false,
        })
    }

    /// How long this widget wants between updates right now: what it asks for,
    /// but never faster than the user's refresh setting.
    pub fn interval(&self) -> Duration {
        self.refresh.interval(&*self.widget)
    }

    pub fn next_due(&self) -> Instant {
        self.refresh.next_due()
    }

    pub fn is_due(&self, now: Instant, tolerance: Duration) -> bool {
        self.refresh.is_due(now, tolerance)
    }

    /// Update the widget and plan its next update from its state *afterwards*
    /// (see `lwo_core::schedule`). Returns whether what it draws changed.
    pub fn update(&mut self, ctx: &Context) -> bool {
        self.refresh.update(&mut *self.widget, ctx)
    }

    pub fn uses_media(&self) -> bool {
        self.widget
            .sources()
            .contains(&lwo_core::widget::Source::Media)
    }

    /// Render the widget (plus the edit-mode highlight) and push it to the screen.
    /// The window is never moved here, so this is safe while it is being dragged.
    pub fn paint(&mut self, style: &Style, opacity: u8, editing: bool) -> Result<()> {
        let (w, h) = (self.surface.width as f32, self.surface.height as f32);
        let widget = &self.widget;
        self.surface.paint(|frame| {
            widget.render(frame, style);
            if editing {
                use lwo_core::widget::Canvas;
                let radius = 10.0 * style.scale;
                let a = style.accent;
                // A faint fill makes every pixel grabbable even with a transparent theme.
                frame.fill_rounded_rect(0.0, 0.0, w, h, radius, Color::new(a.r, a.g, a.b, 0.15));
                frame.stroke_rounded_rect(0.0, 0.0, w, h, radius, (2.0 * style.scale).max(2.0), a);
            }
        })?;

        let size = SIZE {
            cx: self.surface.width,
            cy: self.surface.height,
        };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: opacity,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        // SAFETY: valid window and DC; all pointers outlive the call. A null
        // destination keeps the window where it is.
        unsafe {
            let screen = GetDC(None);
            let r = UpdateLayeredWindow(
                self.hwnd,
                Some(screen),
                None,
                Some(&size),
                Some(self.surface.mem_dc),
                Some(&POINT { x: 0, y: 0 }),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );
            ReleaseDC(None, screen);
            r
        }
    }

    pub fn is_shown(&self) -> bool {
        self.shown
    }

    pub fn set_shown(&mut self, show: bool) {
        if show == self.shown {
            return;
        }
        // SAFETY: valid window handle owned by this panel.
        unsafe {
            let _ = ShowWindow(self.hwnd, if show { SW_SHOWNOACTIVATE } else { SW_HIDE });
        }
        self.shown = show;
    }

    /// Keep above borderless-fullscreen windows that took the top spot meanwhile.
    pub fn raise(&self) {
        // SAFETY: valid window handle.
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }

    /// Normal mode: mouse clicks pass straight through. Edit mode: they don't.
    pub fn set_click_through(&self, through: bool) {
        // SAFETY: valid window handle; only the WS_EX_TRANSPARENT bit is changed.
        unsafe {
            let ex = GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE);
            let bit = WS_EX_TRANSPARENT.0 as isize;
            let next = if through { ex | bit } else { ex & !bit };
            if next != ex {
                SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, next);
            }
        }
    }

    /// Where the window actually is right now.
    pub fn rect(&self) -> Rect {
        let mut r = RECT::default();
        // SAFETY: valid window handle and out-pointer.
        unsafe {
            let _ = GetWindowRect(self.hwnd, &mut r);
        }
        Rect::new(r.left, r.top, r.right, r.bottom)
    }
}

impl Drop for Panel {
    fn drop(&mut self) {
        // SAFETY: the window belongs to this panel and is not used afterwards.
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}
