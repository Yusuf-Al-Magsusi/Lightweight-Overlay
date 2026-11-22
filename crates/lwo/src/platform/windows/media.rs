//! Media source: the system's current media session through the Windows
//! `GlobalSystemMediaTransportControls` API.
//!
//! Event-driven, no polling. A worker thread subscribes to session events,
//! blocks on a channel, and on each (debounced) event reads the session and
//! posts `WM_MEDIA_CHANGED` to the control window. Between events the widget
//! ticks the position forward itself. The worker only exists while the overlay
//! is visible; `stop` ends it.

use std::ffi::c_void;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use lwo_core::widget::MediaSnapshot;
use windows::core::Result;
use windows::Foundation::TypedEventHandler;
use windows::Media::Control::*;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

/// Posted to the control window when the media state may have changed.
pub const WM_MEDIA_CHANGED: u32 = WM_APP + 2;

/// Seconds between 1601-01-01 (Windows epoch) and 1970-01-01.
const WINDOWS_TO_UNIX_SECS: u64 = 11_644_473_600;
/// Ignore position corrections older than this (bogus `LastUpdatedTime`).
const MAX_AGE: Duration = Duration::from_secs(24 * 3600);

enum Msg {
    Refresh,
    Stop,
}

struct Shared {
    snapshot: Mutex<Option<Arc<MediaSnapshot>>>,
    /// Bumped on every start and stop so a worker that is shutting down can't
    /// overwrite newer state.
    generation: AtomicU64,
}

pub struct MediaSource {
    shared: Arc<Shared>,
    tx: Option<mpsc::Sender<Msg>>,
}

impl MediaSource {
    pub fn new() -> Self {
        Self {
            shared: Arc::new(Shared {
                snapshot: Mutex::new(None),
                generation: AtomicU64::new(0),
            }),
            tx: None,
        }
    }

    pub fn snapshot(&self) -> Option<Arc<MediaSnapshot>> {
        self.shared.snapshot.lock().ok().and_then(|g| g.clone())
    }

    /// Start listening. No-op if already running.
    pub fn start(&mut self, notify: HWND) {
        if self.tx.is_some() {
            return;
        }
        let generation = self.shared.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let (tx, rx) = mpsc::channel();
        self.tx = Some(tx.clone());
        let shared = self.shared.clone();
        let hwnd = notify.0 as isize;
        std::thread::spawn(move || worker(generation, shared, rx, tx, hwnd));
    }

    /// Stop listening and forget the last snapshot.
    pub fn stop(&mut self) {
        if let Some(tx) = self.tx.take() {
            self.shared.generation.fetch_add(1, Ordering::SeqCst);
            let _ = tx.send(Msg::Stop);
        }
        if let Ok(mut g) = self.shared.snapshot.lock() {
            *g = None;
        }
    }
}

/// Event subscriptions on the current session.
#[derive(Default)]
struct Subscriptions {
    app_id: Option<String>,
    session: Option<GlobalSystemMediaTransportControlsSession>,
    tokens: [i64; 3],
}

impl Subscriptions {
    fn clear(&mut self) {
        if let Some(s) = self.session.take() {
            let _ = s.RemoveMediaPropertiesChanged(self.tokens[0]);
            let _ = s.RemovePlaybackInfoChanged(self.tokens[1]);
            let _ = s.RemoveTimelinePropertiesChanged(self.tokens[2]);
        }
        self.app_id = None;
    }

    fn attach(
        &mut self,
        session: &GlobalSystemMediaTransportControlsSession,
        tx: &mpsc::Sender<Msg>,
    ) -> Result<()> {
        let t = tx.clone();
        let a = session.MediaPropertiesChanged(&TypedEventHandler::new(move |_, _| {
            let _ = t.send(Msg::Refresh);
            Ok(())
        }))?;
        let t = tx.clone();
        let b = session.PlaybackInfoChanged(&TypedEventHandler::new(move |_, _| {
            let _ = t.send(Msg::Refresh);
            Ok(())
        }))?;
        let t = tx.clone();
        let c = session.TimelinePropertiesChanged(&TypedEventHandler::new(move |_, _| {
            let _ = t.send(Msg::Refresh);
            Ok(())
        }))?;
        self.tokens = [a, b, c];
        self.session = Some(session.clone());
        Ok(())
    }
}

fn worker(
    generation: u64,
    shared: Arc<Shared>,
    rx: mpsc::Receiver<Msg>,
    tx: mpsc::Sender<Msg>,
    hwnd: isize,
) {
    // SAFETY: initialises WinRT for this thread; failure (already initialised) is harmless.
    unsafe {
        let _ = RoInitialize(RO_INIT_MULTITHREADED);
    }
    let Ok(manager) =
        GlobalSystemMediaTransportControlsSessionManager::RequestAsync().and_then(|op| op.join())
    else {
        return;
    };
    let tx_mgr = tx.clone();
    let manager_token = manager
        .CurrentSessionChanged(&TypedEventHandler::new(move |_, _| {
            let _ = tx_mgr.send(Msg::Refresh);
            Ok(())
        }))
        .ok();

    let mut subs = Subscriptions::default();
    'run: loop {
        refresh(generation, &shared, &manager, &mut subs, &tx, hwnd);
        // Wait for an event, then absorb the burst that usually follows it.
        match rx.recv() {
            Ok(Msg::Refresh) => loop {
                match rx.recv_timeout(Duration::from_millis(50)) {
                    Ok(Msg::Refresh) => continue,
                    Ok(Msg::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break 'run,
                    Err(mpsc::RecvTimeoutError::Timeout) => break,
                }
            },
            Ok(Msg::Stop) | Err(_) => break 'run,
        }
    }

    subs.clear();
    if let Some(t) = manager_token {
        let _ = manager.RemoveCurrentSessionChanged(t);
    }
}

fn refresh(
    generation: u64,
    shared: &Shared,
    manager: &GlobalSystemMediaTransportControlsSessionManager,
    subs: &mut Subscriptions,
    tx: &mpsc::Sender<Msg>,
    hwnd: isize,
) {
    let session = manager.GetCurrentSession().ok();
    let app_id = session
        .as_ref()
        .and_then(|s| s.SourceAppUserModelId().ok())
        .map(|h| h.to_string());
    if app_id != subs.app_id {
        subs.clear();
        if let Some(s) = &session {
            if subs.attach(s, tx).is_ok() {
                subs.app_id = app_id;
            }
        }
    }
    let snapshot = session.and_then(|s| read_snapshot(&s).ok()).map(Arc::new);

    if shared.generation.load(Ordering::SeqCst) != generation {
        return; // stopped or restarted while we were reading
    }
    if let Ok(mut g) = shared.snapshot.lock() {
        *g = snapshot;
    }
    // SAFETY: posting to a window handle; fails harmlessly if the window is gone.
    unsafe {
        let _ = PostMessageW(
            Some(HWND(hwnd as *mut c_void)),
            WM_MEDIA_CHANGED,
            WPARAM(0),
            LPARAM(0),
        );
    }
}

fn ticks_to_duration(ticks: i64) -> Duration {
    Duration::from_nanos(ticks.max(0) as u64 * 100)
}

fn windows_now_ticks() -> i64 {
    let since_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    ((since_unix.as_secs() + WINDOWS_TO_UNIX_SECS) * 10_000_000
        + since_unix.subsec_nanos() as u64 / 100) as i64
}

fn read_snapshot(session: &GlobalSystemMediaTransportControlsSession) -> Result<MediaSnapshot> {
    let props = session.TryGetMediaPropertiesAsync()?.join()?;
    let playing = session.GetPlaybackInfo()?.PlaybackStatus()?
        == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing;
    let timeline = session.GetTimelineProperties()?;

    let start = timeline.StartTime()?.Duration;
    let end = timeline.EndTime()?.Duration;
    let duration = ticks_to_duration(end - start);
    let mut position = ticks_to_duration(timeline.Position()?.Duration - start);

    // The reported position was valid at `LastUpdatedTime`; catch up to now.
    if playing {
        let last = timeline.LastUpdatedTime()?.UniversalTime;
        if last > 0 {
            let age = ticks_to_duration(windows_now_ticks() - last);
            if age < MAX_AGE {
                position += age;
            }
        }
    }
    if !duration.is_zero() {
        position = position.min(duration);
    }

    let mut artist = props.Artist()?.to_string();
    if artist.is_empty() {
        artist = props.AlbumArtist()?.to_string();
    }
    Ok(MediaSnapshot {
        title: props.Title()?.to_string(),
        artist,
        album: props.AlbumTitle()?.to_string(),
        position,
        duration,
        playing,
        at: Instant::now(),
    })
}
