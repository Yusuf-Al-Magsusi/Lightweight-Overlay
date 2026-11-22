//! Spike (a): raw Win32 layered window, Direct2D (DC render target) + DirectWrite.
//! Scene: clock, label, progress bar. Redraws once per second, only if the text changed.

use std::ffi::c_void;
use std::time::{SystemTime, UNIX_EPOCH};

use windows::core::{w, Result, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

const W: i32 = 320;
const H: i32 = 130;
const TIMER_ID: usize = 1;

struct Gfx {
    hwnd: HWND,
    mem_dc: HDC,
    rt: ID2D1DCRenderTarget,
    big: IDWriteTextFormat,
    small: IDWriteTextFormat,
    last: String,
}

fn clock_string() -> (String, f32) {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let s = secs % 86_400;
    (
        format!("{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60),
        (secs % 180) as f32 / 180.0,
    )
}

fn color(r: f32, g: f32, b: f32, a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r, g, b, a }
}

impl Gfx {
    fn draw(&mut self) -> Result<()> {
        let (text, progress) = clock_string();
        if text == self.last {
            return Ok(());
        }
        self.last = text.clone();
        unsafe {
            let rc = RECT {
                left: 0,
                top: 0,
                right: W,
                bottom: H,
            };
            self.rt.BindDC(self.mem_dc, &rc)?;
            self.rt.BeginDraw();
            self.rt.Clear(Some(&color(0.0, 0.0, 0.0, 0.0)));
            let bg = self
                .rt
                .CreateSolidColorBrush(&color(0.05, 0.05, 0.08, 0.7), None)?;
            let fg = self
                .rt
                .CreateSolidColorBrush(&color(1.0, 1.0, 1.0, 1.0), None)?;
            let accent = self
                .rt
                .CreateSolidColorBrush(&color(0.3, 0.8, 0.5, 1.0), None)?;
            self.rt.FillRoundedRectangle(
                &D2D1_ROUNDED_RECT {
                    rect: D2D_RECT_F {
                        left: 0.0,
                        top: 0.0,
                        right: W as f32,
                        bottom: H as f32,
                    },
                    radiusX: 10.0,
                    radiusY: 10.0,
                },
                &bg,
            );
            let wide: Vec<u16> = text.encode_utf16().collect();
            self.rt.DrawText(
                &wide,
                &self.big,
                &D2D_RECT_F {
                    left: 14.0,
                    top: 6.0,
                    right: W as f32,
                    bottom: 50.0,
                },
                &fg,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            let label: Vec<u16> = "Now Playing: Song - Artist".encode_utf16().collect();
            self.rt.DrawText(
                &label,
                &self.small,
                &D2D_RECT_F {
                    left: 14.0,
                    top: 58.0,
                    right: W as f32 - 8.0,
                    bottom: 82.0,
                },
                &fg,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            let track = D2D_RECT_F {
                left: 14.0,
                top: 100.0,
                right: W as f32 - 14.0,
                bottom: 108.0,
            };
            self.rt.FillRectangle(&track, &bg);
            let mut filled = track;
            filled.right = track.left + (track.right - track.left) * progress;
            self.rt.FillRectangle(&filled, &accent);
            self.rt.EndDraw(None, None)?;

            let screen = GetDC(None);
            let size = SIZE { cx: W, cy: H };
            let src = POINT { x: 0, y: 0 };
            let dst = POINT { x: 20, y: 20 };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let r = UpdateLayeredWindow(
                self.hwnd,
                Some(screen),
                Some(&dst),
                Some(&size),
                Some(self.mem_dc),
                Some(&src),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );
            ReleaseDC(None, screen);
            r?;
        }
        Ok(())
    }
}

static mut GFX: Option<Gfx> = None;

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_TIMER => {
            #[allow(static_mut_refs)]
            if let Some(g) = unsafe { GFX.as_mut() } {
                let _ = g.draw();
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

fn main() -> Result<()> {
    unsafe {
        let hinst = GetModuleHandleW(None)?;
        let class = w!("SpikeWin32Overlay");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinst.into(),
            lpszClassName: class,
            ..Default::default()
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class,
            PCWSTR::null(),
            WS_POPUP,
            20,
            20,
            W,
            H,
            None,
            None,
            Some(hinst.into()),
            None,
        )?;

        // 32-bit top-down DIB the D2D DC render target draws into.
        let screen = GetDC(None);
        let mem_dc = CreateCompatibleDC(Some(screen));
        ReleaseDC(None, screen);
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: W,
                biHeight: -H,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut c_void = std::ptr::null_mut();
        let dib = CreateDIBSection(Some(mem_dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0)?;
        SelectObject(mem_dc, dib.into());

        let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
        let props = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: 0.0,
            dpiY: 0.0,
            usage: D2D1_RENDER_TARGET_USAGE_NONE,
            minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
        };
        let rt = d2d.CreateDCRenderTarget(&props)?;

        let dw: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
        let mk = |size: f32| -> Result<IDWriteTextFormat> {
            dw.CreateTextFormat(
                w!("Segoe UI"),
                None,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                size,
                w!("en-us"),
            )
        };
        let big = mk(34.0)?;
        let small = mk(16.0)?;

        #[allow(static_mut_refs)]
        {
            GFX = Some(Gfx {
                hwnd,
                mem_dc,
                rt,
                big,
                small,
                last: String::new(),
            });
            GFX.as_mut().unwrap().draw()?;
        }
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        SetTimer(Some(hwnd), TIMER_ID, 1000, None);

        // Blocks in GetMessageW: zero CPU between timer ticks.
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}
