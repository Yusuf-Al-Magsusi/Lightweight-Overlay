//! Dev-only GPU stress for testing the GPU widget with real load.
//!
//! Runs a Direct3D 11 compute shader in a loop on the default hardware adapter
//! for N seconds (default 15): `cargo run --example gpu_load -- 20`.
//! Each dispatch is short and waited on, so it never trips the driver timeout.
//! Not part of the shipped app.

use std::ffi::c_void;
use std::time::{Duration, Instant};

use windows::core::{s, Result};
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::Fxc::D3DCompile;
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_UNKNOWN;

const SHADER: &str = "
RWStructuredBuffer<float> o : register(u0);
[numthreads(256, 1, 1)]
void main(uint3 id : SV_DispatchThreadID) {
    float x = id.x * 0.001f;
    for (int i = 0; i < 3000; i++) { x = sin(x) * cos(x) + x * 0.9999f; }
    o[id.x] = x;
}";

const GROUPS: u32 = 1024;
const THREADS: u32 = GROUPS * 256;

fn main() -> Result<()> {
    let seconds: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(15);

    // SAFETY: straightforward D3D11 setup; every call's result is checked.
    unsafe {
        let mut device = None;
        let mut ctx = None;
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_FLAG(0),
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut ctx),
        )?;
        let (device, ctx) = (device.unwrap(), ctx.unwrap());

        let mut blob = None;
        D3DCompile(
            SHADER.as_ptr().cast(),
            SHADER.len(),
            s!("stress"),
            None,
            None,
            s!("main"),
            s!("cs_5_0"),
            0,
            0,
            &mut blob,
            None,
        )?;
        let blob = blob.unwrap();
        let code =
            std::slice::from_raw_parts(blob.GetBufferPointer().cast::<u8>(), blob.GetBufferSize());
        let mut shader = None;
        device.CreateComputeShader(code, None, Some(&mut shader))?;
        let shader = shader.unwrap();

        let mut buffer = None;
        device.CreateBuffer(
            &D3D11_BUFFER_DESC {
                ByteWidth: THREADS * 4,
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_UNORDERED_ACCESS.0 as u32,
                CPUAccessFlags: 0,
                MiscFlags: D3D11_RESOURCE_MISC_BUFFER_STRUCTURED.0 as u32,
                StructureByteStride: 4,
            },
            None,
            Some(&mut buffer),
        )?;
        let buffer = buffer.unwrap();
        let mut uav = None;
        device.CreateUnorderedAccessView(
            &buffer,
            Some(&D3D11_UNORDERED_ACCESS_VIEW_DESC {
                Format: DXGI_FORMAT_UNKNOWN,
                ViewDimension: D3D11_UAV_DIMENSION_BUFFER,
                Anonymous: D3D11_UNORDERED_ACCESS_VIEW_DESC_0 {
                    Buffer: D3D11_BUFFER_UAV {
                        FirstElement: 0,
                        NumElements: THREADS,
                        Flags: 0,
                    },
                },
            }),
            Some(&mut uav),
        )?;
        let uav = uav.unwrap();

        let mut query = None;
        device.CreateQuery(
            &D3D11_QUERY_DESC {
                Query: D3D11_QUERY_EVENT,
                MiscFlags: 0,
            },
            Some(&mut query),
        )?;
        let query = query.unwrap();

        ctx.CSSetShader(&shader, None);
        ctx.CSSetUnorderedAccessViews(0, 1, Some(&Some(uav)), None);

        println!("stressing the GPU for {seconds} s ...");
        let end = Instant::now() + Duration::from_secs(seconds);
        let mut dispatches = 0u64;
        while Instant::now() < end {
            ctx.Dispatch(GROUPS, 1, 1);
            ctx.End(&query);
            ctx.Flush();
            let mut done = 0u32;
            // Wait for this dispatch to finish before queueing the next.
            while ctx
                .GetData(&query, Some((&mut done as *mut u32).cast::<c_void>()), 4, 0)
                .is_err()
                || done == 0
            {
                std::thread::sleep(Duration::from_millis(1));
            }
            dispatches += 1;
        }
        println!("done, {dispatches} dispatches");
    }
    Ok(())
}
