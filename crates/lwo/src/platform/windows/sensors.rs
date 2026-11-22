//! Polled sensors: CPU load, physical memory, and NVIDIA GPU through NVML.
//!
//! All user-mode and driver-free (NVML ships with the normal NVIDIA driver and
//! is loaded at runtime; if it is missing the GPU widget is simply hidden).
//! CPU and memory use single cheap system calls, so no extra crate is needed.

use std::ffi::c_void;
use std::path::Path;

use lwo_core::widget::{GpuStats, RamStats};
use windows::core::{HSTRING, PCSTR};
use windows::Win32::Foundation::{FreeLibrary, FILETIME, HMODULE};
use windows::Win32::System::LibraryLoader::{
    GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_FLAGS, LOAD_LIBRARY_SEARCH_SYSTEM32,
};
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::System::Threading::GetSystemTimes;

// ------------------------------------------------------------------ CPU

fn filetime(ft: FILETIME) -> u64 {
    (u64::from(ft.dwHighDateTime) << 32) | u64::from(ft.dwLowDateTime)
}

/// (idle, total) system times in 100 ns units. `kernel` already includes idle.
fn system_times() -> Option<(u64, u64)> {
    let (mut idle, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    // SAFETY: pointers to three valid FILETIMEs on the stack.
    unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)).ok()? };
    Some((filetime(idle), filetime(kernel) + filetime(user)))
}

/// Overall CPU load from the change in system times between two polls.
pub struct CpuSampler {
    last: Option<(u64, u64)>,
}

impl CpuSampler {
    pub fn new() -> Self {
        Self { last: None }
    }

    /// Forget the previous sample, for example after the overlay was hidden:
    /// the next `sample` then only records a baseline.
    pub fn reset(&mut self) {
        self.last = None;
    }

    /// Load in percent since the previous call, or `None` on the first call.
    pub fn sample(&mut self) -> Option<f32> {
        let now = system_times()?;
        let prev = self.last.replace(now)?;
        let total = now.1.checked_sub(prev.1)?;
        if total == 0 {
            return None; // timer resolution is about 15 ms; too soon to tell
        }
        let idle = now.0.saturating_sub(prev.0).min(total);
        Some((total - idle) as f32 / total as f32 * 100.0)
    }
}

// --------------------------------------------------------------- Memory

pub fn read_memory() -> Option<RamStats> {
    let mut m = MEMORYSTATUSEX {
        dwLength: size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    // SAFETY: `m` is initialised with its size as the API requires.
    unsafe { GlobalMemoryStatusEx(&mut m).ok()? };
    Some(RamStats {
        used_bytes: m.ullTotalPhys.saturating_sub(m.ullAvailPhys),
        total_bytes: m.ullTotalPhys,
    })
}

// ----------------------------------------------------------------- NVML

type Device = *mut c_void;

#[repr(C)]
struct Utilization {
    gpu: u32,
    memory: u32,
}

#[repr(C)]
struct MemoryInfo {
    total: u64,
    free: u64,
    used: u64,
}

const NVML_SUCCESS: i32 = 0;
const NVML_TEMPERATURE_GPU: u32 = 0;

type FnInit = unsafe extern "C" fn() -> i32;
type FnShutdown = unsafe extern "C" fn() -> i32;
type FnHandleByIndex = unsafe extern "C" fn(u32, *mut Device) -> i32;
type FnTemperature = unsafe extern "C" fn(Device, u32, *mut u32) -> i32;
type FnPower = unsafe extern "C" fn(Device, *mut u32) -> i32;
type FnUtilization = unsafe extern "C" fn(Device, *mut Utilization) -> i32;
type FnMemory = unsafe extern "C" fn(Device, *mut MemoryInfo) -> i32;

/// A loaded NVML with an initialised handle to GPU 0. Dropping shuts NVML
/// down and unloads the library, so a hidden overlay holds no driver resources.
pub struct Nvml {
    lib: HMODULE,
    device: Device,
    shutdown: FnShutdown,
    temperature: FnTemperature,
    power: FnPower,
    utilization: FnUtilization,
    memory: FnMemory,
}

/// Look up one exported function and cast it to its real signature.
macro_rules! symbol {
    ($lib:expr, $name:literal, $ty:ty) => {{
        let addr = GetProcAddress($lib, PCSTR(concat!($name, "\0").as_ptr()))?;
        std::mem::transmute::<unsafe extern "system" fn() -> isize, $ty>(addr)
    }};
}

impl Nvml {
    /// Load NVML from a known location. Returns `None` quietly on any failure.
    ///
    /// Only the system folder (and NVIDIA's standard install folder, by absolute
    /// path) are searched, never the current or application folder, so a
    /// planted `nvml.dll` next to the exe is not loaded. Debug builds also honour
    /// `LWO_NVML_PATH` so tests can use a fake library; release builds never do.
    pub fn open() -> Option<Self> {
        #[cfg(debug_assertions)]
        if let Some(path) = std::env::var_os("LWO_NVML_PATH") {
            return Self::open_path(Path::new(&path));
        }
        let lib = load("nvml.dll", LOAD_LIBRARY_SEARCH_SYSTEM32).or_else(|| {
            let old = r"C:\Program Files\NVIDIA Corporation\NVSMI\nvml.dll";
            load(old, LOAD_LIBRARY_FLAGS(0))
        })?;
        Self::init(lib)
    }

    /// Load from an explicit absolute path (used by tests).
    pub fn open_path(path: &Path) -> Option<Self> {
        let lib = load(&path.to_string_lossy(), LOAD_LIBRARY_FLAGS(0))?;
        Self::init(lib)
    }

    fn init(lib: HMODULE) -> Option<Self> {
        // SAFETY: `lib` is a freshly loaded module; each symbol is cast to the
        // signature declared in nvml.h. Any failure frees the library.
        let result = unsafe { Self::init_inner(lib) };
        if result.is_none() {
            // SAFETY: `lib` was loaded by us and is not used afterwards.
            unsafe {
                let _ = FreeLibrary(lib);
            }
        }
        result
    }

    unsafe fn init_inner(lib: HMODULE) -> Option<Self> {
        // SAFETY (whole body): see `init`.
        unsafe {
            let init: FnInit = symbol!(lib, "nvmlInit_v2", FnInit);
            let shutdown: FnShutdown = symbol!(lib, "nvmlShutdown", FnShutdown);
            let by_index: FnHandleByIndex =
                symbol!(lib, "nvmlDeviceGetHandleByIndex_v2", FnHandleByIndex);
            let temperature = symbol!(lib, "nvmlDeviceGetTemperature", FnTemperature);
            let power = symbol!(lib, "nvmlDeviceGetPowerUsage", FnPower);
            let utilization = symbol!(lib, "nvmlDeviceGetUtilizationRates", FnUtilization);
            let memory = symbol!(lib, "nvmlDeviceGetMemoryInfo", FnMemory);

            if init() != NVML_SUCCESS {
                return None;
            }
            let mut device: Device = std::ptr::null_mut();
            if by_index(0, &mut device) != NVML_SUCCESS || device.is_null() {
                shutdown();
                return None;
            }
            Some(Self {
                lib,
                device,
                shutdown,
                temperature,
                power,
                utilization,
                memory,
            })
        }
    }

    /// One reading from GPU 0. `None` if the essentials (load, temperature,
    /// VRAM) can't be read; power is optional.
    pub fn read(&self) -> Option<GpuStats> {
        let mut util = Utilization { gpu: 0, memory: 0 };
        let mut temp = 0u32;
        let mut mem = MemoryInfo {
            total: 0,
            free: 0,
            used: 0,
        };
        let mut milliwatts = 0u32;
        // SAFETY: `device` is a valid handle from NVML and every out-pointer
        // refers to a live local of the layout NVML expects.
        unsafe {
            if (self.utilization)(self.device, &mut util) != NVML_SUCCESS
                || (self.temperature)(self.device, NVML_TEMPERATURE_GPU, &mut temp) != NVML_SUCCESS
                || (self.memory)(self.device, &mut mem) != NVML_SUCCESS
            {
                return None;
            }
            let power_ok = (self.power)(self.device, &mut milliwatts) == NVML_SUCCESS;
            Some(GpuStats {
                load_percent: util.gpu,
                temperature_c: Some(temp),
                power_watts: power_ok.then(|| milliwatts as f32 / 1000.0),
                vram_used_bytes: mem.used,
                vram_total_bytes: mem.total,
            })
        }
    }
}

impl Drop for Nvml {
    fn drop(&mut self) {
        // SAFETY: shuts down and unloads what `init` set up; not used afterwards.
        unsafe {
            (self.shutdown)();
            let _ = FreeLibrary(self.lib);
        }
    }
}

fn load(name: &str, flags: LOAD_LIBRARY_FLAGS) -> Option<HMODULE> {
    // SAFETY: loading a library by name or absolute path with explicit flags.
    unsafe { LoadLibraryExW(&HSTRING::from(name), None, flags).ok() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_sampler_needs_two_samples_then_reports_a_sane_percentage() {
        let mut s = CpuSampler::new();
        assert!(s.sample().is_none(), "first call only records a baseline");
        // Burn a little CPU so the interval is longer than the timer resolution.
        let t = std::time::Instant::now();
        while t.elapsed() < std::time::Duration::from_millis(60) {
            std::hint::black_box(0);
        }
        let load = s.sample().expect("second sample");
        assert!((0.0..=100.0).contains(&load), "load was {load}");
    }

    #[test]
    fn cpu_sampler_reset_discards_the_baseline() {
        let mut s = CpuSampler::new();
        s.sample();
        s.reset();
        assert!(s.sample().is_none());
    }

    #[test]
    fn memory_reports_plausible_values() {
        let m = read_memory().expect("memory");
        assert!(m.total_bytes > 256 * 1024 * 1024);
        assert!(m.used_bytes > 0 && m.used_bytes <= m.total_bytes);
    }

    /// Path of the fake NVML library built by `cargo build --workspace`.
    fn fake_nvml() -> Option<std::path::PathBuf> {
        // The test exe's folder depends on the Cargo version (`target/debug/deps` or a
        // nested `target/debug/build/...` layout), so search upward for the DLL.
        let exe = std::env::current_exe().ok()?;
        exe.ancestors()
            .skip(1)
            .take(8)
            .map(|dir| dir.join("fake_nvml.dll"))
            .find(|p| p.exists())
    }

    #[test]
    fn missing_library_is_quietly_none() {
        assert!(Nvml::open_path(Path::new(r"C:\definitely\not\here\nvml.dll")).is_none());
    }

    #[test]
    fn library_without_nvml_exports_is_rejected() {
        // kernel32 loads fine but has none of the nvml functions.
        assert!(Nvml::open_path(Path::new(r"C:\Windows\System32\kernel32.dll")).is_none());
    }

    #[test]
    fn reads_values_from_the_fake_nvml() {
        let Some(path) = fake_nvml() else {
            // A silent skip hides a broken test, so CI must fail instead.
            assert!(
                std::env::var_os("CI").is_none(),
                "fake_nvml.dll not found: build the workspace before testing"
            );
            eprintln!("skipped: run `cargo build --workspace` first to build fake_nvml.dll");
            return;
        };
        let nvml = Nvml::open_path(&path).expect("open fake nvml");
        let g = nvml.read().expect("read");
        assert_eq!(g.load_percent, 45);
        assert_eq!(g.temperature_c, Some(61));
        assert_eq!(g.power_watts, Some(180.5));
        assert_eq!(g.vram_used_bytes, 4 * 1024 * 1024 * 1024);
        assert_eq!(g.vram_total_bytes, 16 * 1024 * 1024 * 1024);
    }
}
