//! Dev-only investigation tool for the FPS widget (Milestone 5). Not shipped.
//!
//! Starts a short real-time ETW session on the graphics providers, counts the
//! present-related events per process for N seconds, then stops the session and
//! reports: which processes presented and how often, which event ids fired, how
//! many events Windows dropped, and how much CPU the probe itself used.
//!
//! It only reads Windows' event stream. By default it opens no other process: it
//! prints process ids (match them in Task Manager's Details tab) and marks the
//! foreground window's process. `--names` additionally asks Windows for each
//! process's file name (a limited-rights query handle). Don't use `--names`
//! while a game with anti-cheat is running.
//! Needs Administrator or membership of "Performance Log Users".
//!
//!   cargo run --release --example etw_probe -- 10              DXGI + D3D9 only
//!   cargo run --release --example etw_probe -- 10 --dxgkrnl    plus two kernel graphics events
//!   cargo run --release --example etw_probe -- 10 --dxgkrnl --names

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::Mutex;
use std::time::Duration;

use windows::core::{GUID, PCWSTR, PWSTR};
use windows::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_ALREADY_EXISTS, ERROR_SUCCESS, FILETIME,
};
use windows::Win32::System::Diagnostics::Etw::*;
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetProcessTimes, OpenProcess, QueryFullProcessImageNameW,
    PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

const SESSION: &str = "lwo-etw-probe";
const DXGI: GUID = GUID::from_u128(0xCA11C036_0102_4A2D_A6AD_F03CFED5D3C9);
const D3D9: GUID = GUID::from_u128(0x783ACA0A_790E_4D7F_8451_AA850511C6B9);
const DXGKRNL: GUID = GUID::from_u128(0x802EC45A_1E99_4B83_9920_87C98277BA9D);
/// Event ids believed to mark one presented frame (checked against the histogram this prints).
const DXGI_PRESENT_START: u16 = 42;
const D3D9_PRESENT_START: u16 = 1;
const DXGKRNL_PRESENT_INFO: u16 = 184;
const DXGKRNL_FLIP_INFO: u16 = 168;

#[derive(Default)]
struct Counts {
    /// (provider, event id) -> number of events
    ids: HashMap<(u128, u16), u64>,
    /// (provider, event id, pid) -> number of events, for the frame ids only
    frames: HashMap<(u128, u16, u32), u64>,
    total: u64,
}

static COUNTS: Mutex<Option<Counts>> = Mutex::new(None);

unsafe extern "system" fn on_event(record: *mut EVENT_RECORD) {
    // SAFETY: ETW passes a valid record for the duration of the callback.
    let h = unsafe { &(*record).EventHeader };
    let provider = h.ProviderId.to_u128();
    let id = h.EventDescriptor.Id;
    if let Ok(mut guard) = COUNTS.lock() {
        let c = guard.get_or_insert_with(Counts::default);
        c.total += 1;
        *c.ids.entry((provider, id)).or_default() += 1;
        let frame_event = (provider == DXGI.to_u128() && id == DXGI_PRESENT_START)
            || (provider == D3D9.to_u128() && id == D3D9_PRESENT_START)
            || (provider == DXGKRNL.to_u128()
                && (id == DXGKRNL_PRESENT_INFO || id == DXGKRNL_FLIP_INFO));
        if frame_event {
            *c.frames.entry((provider, id, h.ProcessId)).or_default() += 1;
        }
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// A zeroed `EVENT_TRACE_PROPERTIES` followed by room for the session name.
struct Properties(Vec<u64>);

impl Properties {
    fn new() -> Self {
        let bytes = size_of::<EVENT_TRACE_PROPERTIES>() + 2 * 1024;
        let mut buf = vec![0u64; bytes.div_ceil(8)];
        // SAFETY: the buffer is large enough and 8-byte aligned.
        let p = unsafe { &mut *buf.as_mut_ptr().cast::<EVENT_TRACE_PROPERTIES>() };
        p.Wnode.BufferSize = (buf.len() * 8) as u32;
        p.Wnode.Flags = WNODE_FLAG_TRACED_GUID;
        p.Wnode.ClientContext = 1; // timestamps from QueryPerformanceCounter
        p.LogFileMode = EVENT_TRACE_REAL_TIME_MODE;
        p.BufferSize = 64; // KB per buffer
        p.MinimumBuffers = 32;
        p.MaximumBuffers = 128;
        p.LoggerNameOffset = size_of::<EVENT_TRACE_PROPERTIES>() as u32;
        Self(buf)
    }

    fn ptr(&mut self) -> *mut EVENT_TRACE_PROPERTIES {
        self.0.as_mut_ptr().cast()
    }
}

fn stop_session(name: &[u16]) -> (u32, u32, u32) {
    let mut props = Properties::new();
    // SAFETY: valid properties buffer and NUL-terminated name; handle 0 means "by name".
    unsafe {
        let status = ControlTraceW(
            CONTROLTRACE_HANDLE::default(),
            PCWSTR(name.as_ptr()),
            props.ptr(),
            EVENT_TRACE_CONTROL_STOP,
        );
        let p = &*props.ptr();
        (status.0, p.EventsLost, p.RealTimeBuffersLost)
    }
}

/// Enable a provider, optionally only for specific event ids (Windows 8.1+).
fn enable(session: CONTROLTRACE_HANDLE, provider: &GUID, only: Option<&[u16]>) -> u32 {
    // EVENT_FILTER_EVENT_ID: { BOOLEAN FilterIn; UCHAR Reserved; USHORT Count; USHORT Events[Count]; }
    let mut blob: Vec<u16> = Vec::new();
    if let Some(ids) = only {
        blob.push(1); // FilterIn = TRUE, Reserved = 0
        blob.push(ids.len() as u16);
        blob.extend_from_slice(ids);
    }
    let mut descriptor = EVENT_FILTER_DESCRIPTOR {
        Ptr: blob.as_ptr() as u64,
        Size: (blob.len() * 2) as u32,
        Type: EVENT_FILTER_TYPE_EVENT_ID,
    };
    let params = ENABLE_TRACE_PARAMETERS {
        Version: ENABLE_TRACE_PARAMETERS_VERSION_2,
        EnableFilterDesc: &mut descriptor,
        FilterDescCount: 1,
        ..Default::default()
    };
    // SAFETY: valid session handle and GUID; the filter blob outlives the call.
    unsafe {
        EnableTraceEx2(
            session,
            provider,
            EVENT_CONTROL_CODE_ENABLE_PROVIDER.0,
            TRACE_LEVEL_INFORMATION as u8,
            0,
            0,
            0,
            only.map(|_| &params as *const _),
        )
        .0
    }
}

fn cpu_seconds() -> f64 {
    let mut t = [FILETIME::default(); 4];
    let [creation, exit, kernel, user] = &mut t;
    // SAFETY: four valid out-pointers for the current process.
    unsafe {
        let _ = GetProcessTimes(GetCurrentProcess(), creation, exit, kernel, user);
    }
    let v = |f: &FILETIME| (u64::from(f.dwHighDateTime) << 32 | u64::from(f.dwLowDateTime)) as f64;
    (v(kernel) + v(user)) / 1e7
}

fn process_name(pid: u32) -> String {
    // SAFETY: querying a process image name with the least-privileged access right.
    unsafe {
        let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return "(not accessible)".into();
        };
        let mut buf = [0u16; 520];
        let mut len = buf.len() as u32;
        let ok =
            QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(h);
        match ok {
            Ok(()) => {
                let full = String::from_utf16_lossy(&buf[..len as usize]);
                full.rsplit('\\').next().unwrap_or(&full).to_string()
            }
            Err(_) => "(unknown)".into(),
        }
    }
}

fn provider_name(p: u128) -> &'static str {
    if p == DXGI.to_u128() {
        "DXGI"
    } else if p == D3D9.to_u128() {
        "D3D9"
    } else if p == DXGKRNL.to_u128() {
        "DxgKrnl"
    } else {
        "other"
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let seconds: u64 = args.iter().find_map(|a| a.parse().ok()).unwrap_or(10);
    let with_kernel = args.iter().any(|a| a == "--dxgkrnl");
    let with_names = args.iter().any(|a| a == "--names");
    let name = wide(SESSION);

    stop_session(&name); // clear any leftover from an earlier run
    let mut props = Properties::new();
    let mut session = CONTROLTRACE_HANDLE::default();
    // SAFETY: valid out-handle, name and properties buffer.
    let status = unsafe { StartTraceW(&mut session, PCWSTR(name.as_ptr()), props.ptr()) };
    if status == ERROR_ACCESS_DENIED {
        println!("StartTrace: ACCESS DENIED. This account is neither elevated nor in Performance Log Users.");
        return;
    }
    if status != ERROR_SUCCESS && status != ERROR_ALREADY_EXISTS {
        println!("StartTrace failed with error {}", status.0);
        return;
    }
    println!("StartTrace: OK");
    println!("enable DXGI: error {}", enable(session, &DXGI, None));
    println!("enable D3D9: error {}", enable(session, &D3D9, None));
    if with_kernel {
        let only = [DXGKRNL_PRESENT_INFO, DXGKRNL_FLIP_INFO];
        println!(
            "enable DxgKrnl (2 event ids only): error {}",
            enable(session, &DXGKRNL, Some(&only))
        );
    }

    let mut logger = name.clone();
    let mut logfile = EVENT_TRACE_LOGFILEW {
        LoggerName: PWSTR(logger.as_mut_ptr()),
        ..Default::default()
    };
    logfile.Anonymous1.ProcessTraceMode =
        PROCESS_TRACE_MODE_REAL_TIME | PROCESS_TRACE_MODE_EVENT_RECORD;
    logfile.Anonymous2.EventRecordCallback = Some(on_event);
    logfile.Context = std::ptr::null_mut::<c_void>();
    // SAFETY: `logfile` and the name buffer outlive the trace handle.
    let trace = unsafe { OpenTraceW(&mut logfile) };
    if trace.Value == u64::MAX {
        println!("OpenTrace failed");
        stop_session(&name);
        return;
    }
    println!("OpenTrace: OK, listening for {seconds} s ...");

    let cpu0 = cpu_seconds();
    let consumer = std::thread::spawn(move || {
        // SAFETY: a valid trace handle; returns when the session stops.
        unsafe { ProcessTrace(&[trace], None, None).0 }
    });
    std::thread::sleep(Duration::from_secs(seconds));
    // Read-only window queries: which process owns the window in front right now.
    let mut foreground = 0u32;
    // SAFETY: plain queries; a null window just leaves the id at 0.
    unsafe { GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut foreground)) };
    let (stop_status, events_lost, buffers_lost) = stop_session(&name);
    let _ = consumer.join();
    // SAFETY: closing the handle opened above, after processing finished.
    unsafe {
        let _ = CloseTrace(trace);
    }
    let cpu = cpu_seconds() - cpu0;

    let counts = COUNTS
        .lock()
        .ok()
        .and_then(|mut g| g.take())
        .unwrap_or_default();
    println!("\nstop: error {stop_status}; events lost: {events_lost}; real-time buffers lost: {buffers_lost}");
    println!(
        "events received: {} ({:.0}/s); probe CPU: {:.2} s over {seconds} s = {:.2}% of one core",
        counts.total,
        counts.total as f64 / seconds as f64,
        cpu,
        cpu / seconds as f64 * 100.0
    );

    let mut ids: Vec<_> = counts.ids.iter().collect();
    ids.sort_by(|a, b| b.1.cmp(a.1));
    println!("\nbusiest (provider, event id):");
    for ((p, id), n) in ids.iter().take(10) {
        println!("  {:<8} id {:<4} {:>8} events", provider_name(*p), id, n);
    }

    let mut frames: Vec<_> = counts.frames.iter().collect();
    frames.sort_by(|a, b| b.1.cmp(a.1));
    println!("\nframe events per process (events per second):");
    if frames.is_empty() {
        println!("  none");
    }
    for ((p, id, pid), n) in frames.iter().take(15) {
        println!(
            "  {:<28} pid {:<6} {:<8} id {:<4} {:>7.1}/s{}",
            if with_names {
                process_name(*pid)
            } else {
                String::new()
            },
            pid,
            provider_name(*p),
            id,
            **n as f64 / seconds as f64,
            if *pid == foreground {
                "   <- foreground window"
            } else {
                ""
            }
        );
    }
}
