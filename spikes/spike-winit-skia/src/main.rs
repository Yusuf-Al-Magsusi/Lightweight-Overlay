//! Spike (b): winit window + tiny-skia/fontdue CPU rendering, no graphics API.
//! Presented with UpdateLayeredWindow for per-pixel alpha (the only Windows-specific part).
//! Redraws once per second, only if the text changed; sleeps in between.

use std::ffi::c_void;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Rect, Transform};
use windows::Win32::Foundation::{COLORREF, HWND, POINT, SIZE};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::platform::windows::WindowAttributesExtWindows;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::{Window, WindowId, WindowLevel};

const W: u32 = 320;
const H: u32 = 130;

struct Renderer {
    hwnd: HWND,
    mem_dc: HDC,
    bits: *mut u8,
    pixmap: Pixmap,
    font: fontdue::Font,
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

fn fill_rect(pm: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, c: Color) {
    let mut p = Paint::default();
    p.set_color(c);
    if let Some(r) = Rect::from_xywh(x, y, w, h) {
        pm.fill_path(
            &PathBuilder::from_rect(r),
            &p,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

fn draw_text(pm: &mut Pixmap, font: &fontdue::Font, text: &str, size: f32, x: f32, baseline: f32) {
    let width = pm.width() as i32;
    let height = pm.height() as i32;
    let data = pm.data_mut();
    let mut pen = x;
    for ch in text.chars() {
        let (m, cov) = font.rasterize(ch, size);
        let gx = pen as i32 + m.xmin;
        let gy = baseline as i32 - m.height as i32 - m.ymin;
        for row in 0..m.height {
            for col in 0..m.width {
                let (px, py) = (gx + col as i32, gy + row as i32);
                if px < 0 || py < 0 || px >= width || py >= height {
                    continue;
                }
                let a = cov[row * m.width + col] as u32;
                let i = ((py * width + px) * 4) as usize;
                // White text, premultiplied "over".
                for c in 0..3 {
                    data[i + c] = (a + data[i + c] as u32 * (255 - a) / 255).min(255) as u8;
                }
                data[i + 3] = (a + data[i + 3] as u32 * (255 - a) / 255).min(255) as u8;
            }
        }
        pen += m.advance_width;
    }
}

impl Renderer {
    fn draw(&mut self) {
        let (text, progress) = clock_string();
        if text == self.last {
            return;
        }
        self.last = text.clone();
        self.pixmap.fill(Color::TRANSPARENT);
        let (w, h) = (W as f32, H as f32);
        fill_rect(
            &mut self.pixmap,
            0.0,
            0.0,
            w,
            h,
            Color::from_rgba8(13, 13, 20, 178),
        );
        draw_text(&mut self.pixmap, &self.font, &text, 34.0, 14.0, 46.0);
        draw_text(
            &mut self.pixmap,
            &self.font,
            "Now Playing: Song - Artist",
            16.0,
            14.0,
            76.0,
        );
        fill_rect(
            &mut self.pixmap,
            14.0,
            100.0,
            w - 28.0,
            8.0,
            Color::from_rgba8(13, 13, 20, 200),
        );
        fill_rect(
            &mut self.pixmap,
            14.0,
            100.0,
            (w - 28.0) * progress,
            8.0,
            Color::from_rgba8(77, 204, 128, 255),
        );
        // RGBA premultiplied -> BGRA premultiplied in the DIB.
        let src = self.pixmap.data();
        let dst = unsafe { std::slice::from_raw_parts_mut(self.bits, src.len()) };
        let (dst, _) = dst.as_chunks_mut::<4>();
        let (src, _) = src.as_chunks::<4>();
        for (d, s) in dst.iter_mut().zip(src) {
            *d = [s[2], s[1], s[0], s[3]];
        }
        unsafe {
            let screen = GetDC(None);
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let _ = UpdateLayeredWindow(
                self.hwnd,
                Some(screen),
                Some(&POINT { x: 20, y: 20 }),
                Some(&SIZE {
                    cx: W as i32,
                    cy: H as i32,
                }),
                Some(self.mem_dc),
                Some(&POINT { x: 0, y: 0 }),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );
            ReleaseDC(None, screen);
        }
    }
}

#[derive(Default)]
struct App {
    window: Option<Window>,
    renderer: Option<Renderer>,
    next_tick: Option<Instant>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        let attrs = Window::default_attributes()
            .with_decorations(false)
            .with_visible(false)
            .with_skip_taskbar(true)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_inner_size(LogicalSize::new(W, H))
            .with_position(PhysicalPosition::new(20, 20));
        let window = el.create_window(attrs).expect("window");
        let _ = window.set_cursor_hittest(false); // adds WS_EX_LAYERED | WS_EX_TRANSPARENT
        let RawWindowHandle::Win32(h) = window.window_handle().expect("handle").as_raw() else {
            panic!("not win32");
        };
        let hwnd = HWND(h.hwnd.get() as *mut c_void);
        unsafe {
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(
                hwnd,
                GWL_EXSTYLE,
                ex | (WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0 | WS_EX_LAYERED.0) as isize,
            );
            let screen = GetDC(None);
            let mem_dc = CreateCompatibleDC(Some(screen));
            ReleaseDC(None, screen);
            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: W as i32,
                    biHeight: -(H as i32),
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits: *mut c_void = std::ptr::null_mut();
            let dib = CreateDIBSection(Some(mem_dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0)
                .expect("dib");
            SelectObject(mem_dc, dib.into());
            let font_bytes = std::fs::read(r"C:\Windows\Fonts\segoeuib.ttf").expect("font");
            let font = fontdue::Font::from_bytes(font_bytes, fontdue::FontSettings::default())
                .expect("font parse");
            let mut r = Renderer {
                hwnd,
                mem_dc,
                bits: bits as *mut u8,
                pixmap: Pixmap::new(W, H).expect("pixmap"),
                font,
                last: String::new(),
            };
            r.draw();
            self.renderer = Some(r);
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        self.window = Some(window);
        self.next_tick = Some(Instant::now() + Duration::from_secs(1));
        el.set_control_flow(ControlFlow::WaitUntil(self.next_tick.unwrap()));
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, ev: WindowEvent) {
        if let WindowEvent::CloseRequested = ev {
            el.exit();
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        if let Some(t) = self.next_tick {
            if Instant::now() >= t {
                if let Some(r) = self.renderer.as_mut() {
                    r.draw();
                }
                self.next_tick = Some(Instant::now() + Duration::from_secs(1));
            }
            el.set_control_flow(ControlFlow::WaitUntil(self.next_tick.unwrap()));
        }
    }
}

fn main() {
    let el = EventLoop::new().expect("event loop");
    el.run_app(&mut App::default()).expect("run");
}
