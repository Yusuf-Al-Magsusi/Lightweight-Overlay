//! Dev-only fake `nvml.dll`, so the GPU widget and the NVML loading code can be
//! tested on machines without an NVIDIA GPU. Not shipped.
//!
//! Fixed values: load 45 %, 61 C, 180.5 W, 4 of 16 GiB VRAM. With the
//! environment variable `FAKE_NVML_VARY` set, the load and temperature change
//! every second so a running overlay visibly updates.
//!
//! Struct layouts and function signatures follow NVIDIA's `nvml.h`.

#![allow(non_snake_case, clippy::missing_safety_doc)]

use std::ffi::c_void;
use std::time::{SystemTime, UNIX_EPOCH};

const GIB: u64 = 1024 * 1024 * 1024;

#[repr(C)]
pub struct Utilization {
    gpu: u32,
    memory: u32,
}

#[repr(C)]
pub struct MemoryInfo {
    total: u64,
    free: u64,
    used: u64,
}

fn seconds() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as u32)
        .unwrap_or(0)
}

fn vary() -> bool {
    std::env::var_os("FAKE_NVML_VARY").is_some()
}

#[no_mangle]
pub extern "C" fn nvmlInit_v2() -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn nvmlShutdown() -> i32 {
    0
}

#[no_mangle]
pub unsafe extern "C" fn nvmlDeviceGetHandleByIndex_v2(
    index: u32,
    device: *mut *mut c_void,
) -> i32 {
    if index != 0 {
        return 6; // NVML_ERROR_NOT_FOUND
    }
    // SAFETY: the caller passes a valid out-pointer.
    unsafe { *device = std::ptr::dangling_mut::<c_void>() };
    0
}

#[no_mangle]
pub unsafe extern "C" fn nvmlDeviceGetTemperature(
    _device: *mut c_void,
    _sensor: u32,
    temp: *mut u32,
) -> i32 {
    // SAFETY: the caller passes a valid out-pointer.
    unsafe { *temp = if vary() { 40 + seconds() % 40 } else { 61 } };
    0
}

#[no_mangle]
pub unsafe extern "C" fn nvmlDeviceGetPowerUsage(
    _device: *mut c_void,
    milliwatts: *mut u32,
) -> i32 {
    // SAFETY: the caller passes a valid out-pointer.
    unsafe { *milliwatts = 180_500 };
    0
}

#[no_mangle]
pub unsafe extern "C" fn nvmlDeviceGetUtilizationRates(
    _device: *mut c_void,
    out: *mut Utilization,
) -> i32 {
    // SAFETY: the caller passes a valid out-pointer.
    unsafe {
        (*out).gpu = if vary() { (seconds() % 15) * 7 } else { 45 };
        (*out).memory = 30;
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn nvmlDeviceGetMemoryInfo(
    _device: *mut c_void,
    out: *mut MemoryInfo,
) -> i32 {
    // SAFETY: the caller passes a valid out-pointer.
    unsafe {
        (*out).total = 16 * GIB;
        (*out).used = 4 * GIB;
        (*out).free = 12 * GIB;
    }
    0
}
