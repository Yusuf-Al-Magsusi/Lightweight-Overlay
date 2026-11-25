//! Dev-only "fake game" with a known frame rate, to check the FPS widget. Not shipped.
//!
//! Opens a small window with a Direct3D 11 swap chain and presents at the
//! requested rate for a number of seconds, then prints the rate it really
//! achieved as `actual_fps=...` (the ground truth).
//!
//!   cargo run --release --example present_rate -- 100 12
//!   cargo run --release --example present_rate -- 60 20 --front   (also asks to be the front window)

use std::time::{Duration, Instant};

use windows::core::{w, Result};
use windows::Win32::Foundation::{HINSTANCE, HMODULE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::{
    IDXGISwapChain, DXGI_PRESENT, DXGI_SWAP_CHAIN_DESC, DXGI_SWAP_EFFECT_FLIP_DISCARD,
    DXGI_USAGE_RENDER_TARGET_OUTPUT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    // SAFETY: default handling.
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut numbers = args.iter().filter_map(|a| a.parse::<u32>().ok());
    let target = numbers.next().unwrap_or(60).clamp(1, 1000);
    let seconds = numbers.next().unwrap_or(10).clamp(1, 600);
    let front = args.iter().any(|a| a == "--front");

    // SAFETY: standard window and Direct3D 11 setup with every result checked.
    unsafe {
        let hinst: HINSTANCE = GetModuleHandleW(None)?.into();
        let class = w!("lwo-present-rate");
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: hinst,
            lpszClassName: class,
            ..Default::default()
        });
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            w!("present_rate test"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            600,
            200,
            360,
            240,
            None,
            None,
            Some(hinst),
            None,
        )?;
        if front {
            let _ = SetForegroundWindow(hwnd);
        }

        let desc = DXGI_SWAP_CHAIN_DESC {
            BufferDesc: windows::Win32::Graphics::Dxgi::Common::DXGI_MODE_DESC {
                Width: 320,
                Height: 200,
                Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                ..Default::default()
            },
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: 2,
            OutputWindow: hwnd,
            Windowed: true.into(),
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
            Flags: 0,
        };
        let mut chain: Option<IDXGISwapChain> = None;
        let mut device = None;
        let mut context = None;
        D3D11CreateDeviceAndSwapChain(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_FLAG(0),
            None,
            D3D11_SDK_VERSION,
            Some(&desc),
            Some(&mut chain),
            Some(&mut device),
            None,
            Some(&mut context),
        )?;
        let (chain, device, context) = (chain.unwrap(), device.unwrap(), context.unwrap());
        let back: ID3D11Texture2D = chain.GetBuffer(0)?;
        let mut view = None;
        device.CreateRenderTargetView(&back, None, Some(&mut view))?;
        let view = view.unwrap();

        println!(
            "pid={} presenting at {target} fps for {seconds} s",
            std::process::id()
        );
        let frame = Duration::from_secs_f64(1.0 / f64::from(target));
        let start = Instant::now();
        let mut next = start;
        let mut presented = 0u64;
        while start.elapsed() < Duration::from_secs(u64::from(seconds)) {
            let shade = (presented % 120) as f32 / 120.0;
            context.ClearRenderTargetView(&view, &[shade, 0.3, 0.6, 1.0]);
            if chain.Present(0, DXGI_PRESENT(0)).is_ok() {
                presented += 1;
            }
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            // Pace precisely: sleep most of the wait, then spin the last 2 ms.
            next += frame;
            loop {
                let left = next.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    break;
                }
                if left > Duration::from_millis(2) {
                    std::thread::sleep(left - Duration::from_millis(2));
                } else {
                    std::hint::spin_loop();
                }
            }
        }
        let actual = presented as f64 / start.elapsed().as_secs_f64();
        println!("frames={presented}");
        println!("actual_fps={actual:.2}");
    }
    Ok(())
}
