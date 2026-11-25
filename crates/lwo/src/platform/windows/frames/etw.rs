//! Frame events read directly from Windows with ETW (Event Tracing for
//! Windows), the method PresentMon uses. User-mode: nothing is injected, hooked
//! or opened in any game.
//!
//! A real-time session enables only the three "a frame was presented" events,
//! so the volume stays small (see docs/fps-investigation.md). A thread blocks in
//! `ProcessTrace` and feeds `lwo_core::fps::FrameCounter`. The session belongs
//! to the system, so it must be stopped: `Drop` does that, and a leftover from
//! a killed run is stopped before a new one starts.

use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use lwo_core::fps::{FrameCounter, FrameOrigin};
use lwo_core::APP_NAME;
use windows::core::{GUID, PCWSTR, PWSTR};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Diagnostics::Etw::*;
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::Win32::System::Threading::GetCurrentProcessId;

use super::FrameSource;

const DXGI: GUID = GUID::from_u128(0xCA11C036_0102_4A2D_A6AD_F03CFED5D3C9);
const D3D9: GUID = GUID::from_u128(0x783ACA0A_790E_4D7F_8451_AA850511C6B9);
const DXGKRNL: GUID = GUID::from_u128(0x802EC45A_1E99_4B83_9920_87C98277BA9D);
const DXGI_PRESENT_START: u16 = 42;
const D3D9_PRESENT_START: u16 = 1;
/// Kernel present: also seen for programs that bypass the DXGI runtime events.
const DXGKRNL_PRESENT_INFO: u16 = 184;

/// The frame rate is counted over this long...
const WINDOW: Duration = Duration::from_millis(1000);
/// ...ending this long ago, so that its events have all arrived.
///
/// Left alone, Windows delivers events about once a second and, measured on the
/// dev PC, they arrive up to 2.2 s after the frame (a first version assumed 1.2 s
/// and read 30 fps as 12 to 18, which the accuracy test caught). So the session
/// is asked to flush every `FLUSH_EVERY`, which cut the lag to 0.36 s typical
/// and 0.58 s worst case; this delay leaves a margin above that.
const DELIVERY_DELAY: Duration = Duration::from_millis(800);
/// How often the buffered events are pushed out. The thread doing it exists only
/// while the FPS widget is on screen.
const FLUSH_EVERY: Duration = Duration::from_millis(200);

type Shared = Mutex<FrameCounter>;

fn origin_of(provider: &GUID, id: u16) -> Option<FrameOrigin> {
    if (*provider == DXGI && id == DXGI_PRESENT_START)
        || (*provider == D3D9 && id == D3D9_PRESENT_START)
    {
        Some(FrameOrigin::Runtime)
    } else if *provider == DXGKRNL && id == DXGKRNL_PRESENT_INFO {
        Some(FrameOrigin::Kernel)
    } else {
        None
    }
}

unsafe extern "system" fn on_event(record: *mut EVENT_RECORD) {
    // SAFETY: ETW passes a valid record; `UserContext` is the `Shared` kept alive
    // until after the consumer thread has finished (see `Drop`).
    unsafe {
        let r = &*record;
        let header = &r.EventHeader;
        let Some(origin) = origin_of(&header.ProviderId, header.EventDescriptor.Id) else {
            return;
        };
        let shared = &*(r.UserContext as *const Shared);
        if let Ok(mut counter) = shared.lock() {
            counter.record(header.ProcessId, origin, header.TimeStamp.max(0) as u64);
        }
    }
}

/// Sessions are machine-wide, so the name includes the Windows logon session:
/// two signed-in users never stop each other's.
fn session_name() -> Vec<u16> {
    let mut logon = 0u32;
    // SAFETY: valid out-pointer for our own process id.
    unsafe {
        let _ = ProcessIdToSessionId(GetCurrentProcessId(), &mut logon);
    }
    format!("{APP_NAME}-fps-{logon}")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect()
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
        p.MinimumBuffers = 4;
        p.MaximumBuffers = 24;
        p.FlushTimer = 1; // seconds
        p.LoggerNameOffset = size_of::<EVENT_TRACE_PROPERTIES>() as u32;
        Self(buf)
    }

    fn ptr(&mut self) -> *mut EVENT_TRACE_PROPERTIES {
        self.0.as_mut_ptr().cast()
    }
}

/// Stop the session with our name, wherever it came from. Failing because there
/// is none is normal, so the status is ignored.
fn stop_by_name(name: &[u16]) {
    let mut props = Properties::new();
    // SAFETY: valid properties and NUL-terminated name; a zero handle means "by name".
    unsafe {
        let _ = ControlTraceW(
            CONTROLTRACE_HANDLE::default(),
            PCWSTR(name.as_ptr()),
            props.ptr(),
            EVENT_TRACE_CONTROL_STOP,
        );
    }
}

/// Ask Windows to deliver whatever is sitting in the session's buffers now.
fn flush(name: &[u16]) {
    let mut props = Properties::new();
    // SAFETY: valid properties and NUL-terminated name.
    unsafe {
        let _ = ControlTraceW(
            CONTROLTRACE_HANDLE::default(),
            PCWSTR(name.as_ptr()),
            props.ptr(),
            EVENT_TRACE_CONTROL_FLUSH,
        );
    }
}

pub fn stop_leftover_session() {
    stop_by_name(&session_name());
}

/// Enable a provider for exactly one event id.
fn enable_one_event(session: CONTROLTRACE_HANDLE, provider: &GUID, event: u16) -> bool {
    // EVENT_FILTER_EVENT_ID { BOOLEAN FilterIn; UCHAR Reserved; USHORT Count; USHORT Events[1]; }
    let blob: [u16; 3] = [1, 1, event];
    let mut descriptor = EVENT_FILTER_DESCRIPTOR {
        Ptr: blob.as_ptr() as u64,
        Size: size_of_val(&blob) as u32,
        Type: EVENT_FILTER_TYPE_EVENT_ID,
    };
    let params = ENABLE_TRACE_PARAMETERS {
        Version: ENABLE_TRACE_PARAMETERS_VERSION_2,
        EnableFilterDesc: &mut descriptor,
        FilterDescCount: 1,
        ..Default::default()
    };
    // SAFETY: valid session and GUID; the filter outlives the call.
    unsafe {
        EnableTraceEx2(
            session,
            provider,
            EVENT_CONTROL_CODE_ENABLE_PROVIDER.0,
            TRACE_LEVEL_INFORMATION as u8,
            0,
            0,
            0,
            Some(&params),
        ) == ERROR_SUCCESS
    }
}

fn now_ticks() -> u64 {
    let mut t = 0i64;
    // SAFETY: valid out-pointer.
    unsafe {
        let _ = QueryPerformanceCounter(&mut t);
    }
    t.max(0) as u64
}

pub struct EtwFrames {
    name: Vec<u16>,
    trace: PROCESSTRACE_HANDLE,
    shared: Arc<Shared>,
    /// The extra reference handed to the callback; released after the consumer ends.
    callback_ref: *const Shared,
    consumer: Option<JoinHandle<()>>,
    /// Pushes buffered events out regularly; dropping the sender ends it.
    flusher: Option<(mpsc::Sender<()>, JoinHandle<()>)>,
}

impl EtwFrames {
    pub fn start() -> Option<Self> {
        let mut frequency = 0i64;
        // SAFETY: valid out-pointer.
        unsafe { QueryPerformanceFrequency(&mut frequency).ok()? };

        let name = session_name();
        stop_by_name(&name); // a leftover from a killed run would block the name
        let mut props = Properties::new();
        let mut session = CONTROLTRACE_HANDLE::default();
        // SAFETY: valid out-handle, name and properties.
        let status = unsafe { StartTraceW(&mut session, PCWSTR(name.as_ptr()), props.ptr()) };
        if status != ERROR_SUCCESS {
            return None;
        }

        let enabled = enable_one_event(session, &DXGI, DXGI_PRESENT_START)
            && enable_one_event(session, &D3D9, D3D9_PRESENT_START)
            && enable_one_event(session, &DXGKRNL, DXGKRNL_PRESENT_INFO);
        if !enabled {
            stop_by_name(&name);
            return None;
        }

        let shared: Arc<Shared> = Arc::new(Mutex::new(FrameCounter::new(
            frequency as u64,
            WINDOW,
            DELIVERY_DELAY,
        )));
        let callback_ref = Arc::into_raw(shared.clone());

        let mut logger = name.clone();
        let mut logfile = EVENT_TRACE_LOGFILEW {
            LoggerName: PWSTR(logger.as_mut_ptr()),
            Context: callback_ref as *mut _,
            ..Default::default()
        };
        // Raw timestamps keep event times on the same clock as `now_ticks`.
        logfile.Anonymous1.ProcessTraceMode = PROCESS_TRACE_MODE_REAL_TIME
            | PROCESS_TRACE_MODE_EVENT_RECORD
            | PROCESS_TRACE_MODE_RAW_TIMESTAMP;
        logfile.Anonymous2.EventRecordCallback = Some(on_event);
        // SAFETY: `logfile` and its name buffer are valid for the call.
        let trace = unsafe { OpenTraceW(&mut logfile) };
        if trace.Value == u64::MAX {
            stop_by_name(&name);
            // SAFETY: the callback was never registered, so the reference is ours.
            unsafe { drop(Arc::from_raw(callback_ref)) };
            return None;
        }

        let consumer = std::thread::spawn(move || {
            // SAFETY: a valid trace handle; blocks until the session stops.
            unsafe {
                let _ = ProcessTrace(&[trace], None, None);
            }
        });

        let (stop_flushing, stopped) = mpsc::channel::<()>();
        let flush_name = name.clone();
        let flusher = std::thread::spawn(move || {
            // Waits on the channel, so it ends at once when the sender is dropped.
            while stopped.recv_timeout(FLUSH_EVERY) == Err(mpsc::RecvTimeoutError::Timeout) {
                flush(&flush_name);
            }
        });

        Some(Self {
            name,
            trace,
            shared,
            callback_ref,
            consumer: Some(consumer),
            flusher: Some((stop_flushing, flusher)),
        })
    }
}

impl FrameSource for EtwFrames {
    fn fps(&mut self, pid: u32) -> Option<f32> {
        let now = now_ticks();
        let mut counter = self.shared.lock().ok()?;
        counter.prune(now);
        counter.fps(pid, now)
    }

    fn events_lost(&mut self) -> u32 {
        let mut props = Properties::new();
        // SAFETY: querying our own session by name.
        unsafe {
            let status = ControlTraceW(
                CONTROLTRACE_HANDLE::default(),
                PCWSTR(self.name.as_ptr()),
                props.ptr(),
                EVENT_TRACE_CONTROL_QUERY,
            );
            if status != ERROR_SUCCESS {
                return 0;
            }
            let p = &*props.ptr();
            p.EventsLost.saturating_add(p.RealTimeBuffersLost)
        }
    }
}

impl Drop for EtwFrames {
    fn drop(&mut self) {
        if let Some((stop, thread)) = self.flusher.take() {
            drop(stop);
            let _ = thread.join();
        }
        // Stopping the session makes `ProcessTrace` return.
        stop_by_name(&self.name);
        if let Some(thread) = self.consumer.take() {
            let _ = thread.join();
        }
        // SAFETY: processing has finished, so nothing uses the handle or the
        // callback's reference any more.
        unsafe {
            let _ = CloseTrace(self.trace);
            drop(Arc::from_raw(self.callback_ref));
        }
    }
}
