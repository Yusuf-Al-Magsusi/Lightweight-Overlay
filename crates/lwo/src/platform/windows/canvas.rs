//! Direct2D + DirectWrite implementation of the core `Canvas` trait.
//! Draws into a 32-bit premultiplied-alpha DIB through a DC render target; the
//! window shows the result with `UpdateLayeredWindow`.
//!
//! The factories are created once (`Gfx`) and shared by every surface, so
//! each widget window costs only its bitmap and render target.

use std::ffi::c_void;

use lwo_core::widget::{Canvas, Color, TextMeasure};
use windows::core::{w, Result, HSTRING};
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::*;

const HUGE: f32 = 10_000.0;

fn d2d_color(c: Color) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: c.r,
        g: c.g,
        b: c.b,
        a: c.a,
    }
}

/// Text measuring only needs DirectWrite, so sizes can be computed before any window exists.
#[derive(Clone)]
pub struct Measurer {
    dw: IDWriteFactory,
}

impl Measurer {
    pub fn new() -> Result<Self> {
        // SAFETY: plain COM factory creation with valid arguments.
        let dw = unsafe { DWriteCreateFactory::<IDWriteFactory>(DWRITE_FACTORY_TYPE_SHARED)? };
        Ok(Self { dw })
    }

    fn format(&self, font: &str, size: f32) -> Result<IDWriteTextFormat> {
        // SAFETY: valid factory and NUL-terminated wide strings.
        unsafe {
            self.dw.CreateTextFormat(
                &HSTRING::from(font),
                None,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                size,
                w!("en-us"),
            )
        }
    }

    fn try_measure(&self, text: &str, font: &str, size: f32) -> Result<(f32, f32)> {
        let format = self.format(font, size)?;
        let wide: Vec<u16> = text.encode_utf16().collect();
        // SAFETY: valid factory, format and UTF-16 slice.
        unsafe {
            let layout = self.dw.CreateTextLayout(&wide, &format, HUGE, HUGE)?;
            let mut m = DWRITE_TEXT_METRICS::default();
            layout.GetMetrics(&mut m)?;
            Ok((m.widthIncludingTrailingWhitespace, m.height))
        }
    }
}

impl TextMeasure for Measurer {
    fn measure_text(&self, text: &str, font: &str, size: f32) -> (f32, f32) {
        // Fall back to a rough estimate rather than failing to draw at all.
        self.try_measure(text, font, size)
            .unwrap_or((text.chars().count() as f32 * size * 0.6, size * 1.3))
    }
}

/// The graphics factories, created once and shared by all surfaces.
#[derive(Clone)]
pub struct Gfx {
    d2d: ID2D1Factory,
    pub measurer: Measurer,
}

impl Gfx {
    pub fn new() -> Result<Self> {
        // SAFETY: plain factory creation with valid arguments.
        let d2d: ID2D1Factory =
            unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)? };
        Ok(Self {
            d2d,
            measurer: Measurer::new()?,
        })
    }
}

/// Owns the DIB and render target for one widget window.
pub struct Surface {
    pub width: i32,
    pub height: i32,
    pub mem_dc: HDC,
    measurer: Measurer,
    rt: ID2D1DCRenderTarget,
    dib: HBITMAP,
}

impl Surface {
    pub fn new(gfx: &Gfx, width: i32, height: i32) -> Result<Self> {
        // SAFETY: standard GDI/Direct2D setup; every handle is checked by `?` and kept alive in `Self`.
        unsafe {
            let screen = GetDC(None);
            let mem_dc = CreateCompatibleDC(Some(screen));
            ReleaseDC(None, screen);
            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height, // top-down
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits: *mut c_void = std::ptr::null_mut();
            let dib = match CreateDIBSection(Some(mem_dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0)
            {
                Ok(d) => d,
                Err(e) => {
                    let _ = DeleteDC(mem_dc);
                    return Err(e);
                }
            };
            SelectObject(mem_dc, dib.into());

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
            let rt = match gfx.d2d.CreateDCRenderTarget(&props) {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = DeleteDC(mem_dc);
                    let _ = DeleteObject(dib.into());
                    return Err(e);
                }
            };
            Ok(Self {
                width,
                height,
                mem_dc,
                measurer: gfx.measurer.clone(),
                rt,
                dib,
            })
        }
    }

    /// Clear to transparent, run `draw`, then flush. The result is in `mem_dc`.
    pub fn paint(&mut self, draw: impl FnOnce(&mut Frame<'_>)) -> Result<()> {
        let rc = RECT {
            left: 0,
            top: 0,
            right: self.width,
            bottom: self.height,
        };
        // SAFETY: `mem_dc` and `rt` are valid; BeginDraw/EndDraw are paired.
        unsafe {
            self.rt.BindDC(self.mem_dc, &rc)?;
            self.rt.BeginDraw();
            self.rt.Clear(Some(&D2D1_COLOR_F::default()));
        }
        let mut frame = Frame {
            rt: &self.rt,
            measurer: &self.measurer,
        };
        draw(&mut frame);
        // SAFETY: paired with BeginDraw above.
        unsafe { self.rt.EndDraw(None, None) }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        // SAFETY: both handles were created in `new` and are not used afterwards.
        // Deleting the DC first releases the bitmap selected into it.
        unsafe {
            let _ = DeleteDC(self.mem_dc);
            let _ = DeleteObject(self.dib.into());
        }
    }
}

/// One frame being drawn.
pub struct Frame<'a> {
    rt: &'a ID2D1DCRenderTarget,
    measurer: &'a Measurer,
}

impl TextMeasure for Frame<'_> {
    fn measure_text(&self, text: &str, font: &str, size: f32) -> (f32, f32) {
        self.measurer.measure_text(text, font, size)
    }
}

fn rounded(x: f32, y: f32, w: f32, h: f32, radius: f32) -> D2D1_ROUNDED_RECT {
    D2D1_ROUNDED_RECT {
        rect: D2D_RECT_F {
            left: x,
            top: y,
            right: x + w,
            bottom: y + h,
        },
        radiusX: radius,
        radiusY: radius,
    }
}

impl Canvas for Frame<'_> {
    fn fill_rounded_rect(&mut self, x: f32, y: f32, w: f32, h: f32, radius: f32, color: Color) {
        // SAFETY: called between BeginDraw and EndDraw with a valid render target.
        unsafe {
            if let Ok(brush) = self.rt.CreateSolidColorBrush(&d2d_color(color), None) {
                self.rt
                    .FillRoundedRectangle(&rounded(x, y, w, h, radius), &brush);
            }
        }
    }

    fn stroke_rounded_rect(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        thickness: f32,
        color: Color,
    ) {
        // Strokes are centred on the path, so inset by half the thickness to stay inside.
        let half = thickness / 2.0;
        let rect = rounded(
            x + half,
            y + half,
            w - thickness,
            h - thickness,
            (radius - half).max(0.0),
        );
        // SAFETY: called between BeginDraw and EndDraw with a valid render target.
        unsafe {
            if let Ok(brush) = self.rt.CreateSolidColorBrush(&d2d_color(color), None) {
                self.rt.DrawRoundedRectangle(&rect, &brush, thickness, None);
            }
        }
    }

    fn draw_text_trimmed(
        &mut self,
        text: &str,
        font: &str,
        size: f32,
        x: f32,
        y: f32,
        max_width: f32,
        color: Color,
    ) {
        let Ok(format) = self.measurer.format(font, size) else {
            return;
        };
        let wide: Vec<u16> = text.encode_utf16().collect();
        // SAFETY: called between BeginDraw and EndDraw with a valid render target;
        // the trimming sign is created from the same format and factory.
        unsafe {
            let _ = format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
            let trimming = DWRITE_TRIMMING {
                granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                delimiter: 0,
                delimiterCount: 0,
            };
            if let Ok(sign) = self.measurer.dw.CreateEllipsisTrimmingSign(&format) {
                let _ = format.SetTrimming(&trimming, &sign);
            }
            if let Ok(brush) = self.rt.CreateSolidColorBrush(&d2d_color(color), None) {
                self.rt.DrawText(
                    &wide,
                    &format,
                    &D2D_RECT_F {
                        left: x,
                        top: y,
                        right: x + max_width,
                        bottom: y + HUGE,
                    },
                    &brush,
                    D2D1_DRAW_TEXT_OPTIONS_CLIP,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }
        }
    }

    fn draw_text(&mut self, text: &str, font: &str, size: f32, x: f32, y: f32, color: Color) {
        let Ok(format) = self.measurer.format(font, size) else {
            return;
        };
        let wide: Vec<u16> = text.encode_utf16().collect();
        // SAFETY: called between BeginDraw and EndDraw with a valid render target.
        unsafe {
            if let Ok(brush) = self.rt.CreateSolidColorBrush(&d2d_color(color), None) {
                self.rt.DrawText(
                    &wide,
                    &format,
                    &D2D_RECT_F {
                        left: x,
                        top: y,
                        right: x + HUGE,
                        bottom: y + HUGE,
                    },
                    &brush,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }
        }
    }
}
