//! Dev-only fake media player for testing the Now Playing widget without a real
//! music app. Registers a Windows media session and walks through a script:
//!
//!   0 s   playing "Test Song" at 0:30 of 3:00
//!   10 s  paused at 0:40
//!   20 s  playing a track with a very long title at 1:40 of 5:00
//!   30 s  exits (the session disappears)
//!
//! Run: `cargo run --example fake_media` (add `-- --forever` to keep playing
//! indefinitely). Not part of the shipped app.

use std::time::{Duration, Instant};

use windows::core::{factory, w, Result, HSTRING};
use windows::Foundation::TimeSpan;
use windows::Media::*;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::WinRT::ISystemMediaTransportControlsInterop;
use windows::Win32::UI::WindowsAndMessaging::*;

fn secs(s: i64) -> TimeSpan {
    TimeSpan {
        Duration: s * 10_000_000,
    }
}

fn publish(
    smtc: &SystemMediaTransportControls,
    title: &str,
    artist: &str,
    album: &str,
    status: MediaPlaybackStatus,
    position: i64,
    duration: i64,
) -> Result<()> {
    let updater = smtc.DisplayUpdater()?;
    updater.SetType(MediaPlaybackType::Music)?;
    let music = updater.MusicProperties()?;
    music.SetTitle(&HSTRING::from(title))?;
    music.SetArtist(&HSTRING::from(artist))?;
    music.SetAlbumTitle(&HSTRING::from(album))?;
    updater.Update()?;
    smtc.SetPlaybackStatus(status)?;

    let tl = SystemMediaTransportControlsTimelineProperties::new()?;
    tl.SetStartTime(secs(0))?;
    tl.SetMinSeekTime(secs(0))?;
    tl.SetEndTime(secs(duration))?;
    tl.SetMaxSeekTime(secs(duration))?;
    tl.SetPosition(secs(position))?;
    smtc.UpdateTimelineProperties(&tl)?;
    println!("published: {title:?} {status:?} {position}/{duration}s");
    Ok(())
}

fn main() -> Result<()> {
    // SAFETY: a plain hidden STATIC window, only used as the media session owner.
    let hwnd: HWND = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("STATIC"),
            w!("fake media player"),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            None,
            None,
            None,
            None,
        )?
    };
    let interop = factory::<SystemMediaTransportControls, ISystemMediaTransportControlsInterop>()?;
    // SAFETY: valid window handle owned by this thread.
    let smtc: SystemMediaTransportControls = unsafe { interop.GetForWindow(hwnd)? };
    smtc.SetIsEnabled(true)?;
    smtc.SetIsPlayEnabled(true)?;
    smtc.SetIsPauseEnabled(true)?;

    let start = Instant::now();
    publish(
        &smtc,
        "Test Song",
        "Test Artist",
        "Test Album",
        MediaPlaybackStatus::Playing,
        30,
        180,
    )?;
    // `--forever`: keep playing "Test Song" until killed (used for measurements).
    let forever = std::env::args().any(|a| a == "--forever");
    let mut step = if forever { 99 } else { 0 };
    loop {
        let t = start.elapsed();
        if step == 0 && t >= Duration::from_secs(10) {
            publish(
                &smtc,
                "Test Song",
                "Test Artist",
                "Test Album",
                MediaPlaybackStatus::Paused,
                40,
                180,
            )?;
            step = 1;
        } else if step == 1 && t >= Duration::from_secs(20) {
            publish(
                &smtc,
                "A Really Very Extremely Long Song Title That Cannot Possibly Fit In The Overlay",
                "An Artist With A Similarly Long Name Collaborating With Many Other Artists",
                "Album",
                MediaPlaybackStatus::Playing,
                100,
                300,
            )?;
            step = 2;
        } else if step == 2 && t >= Duration::from_secs(30) {
            break;
        }
        // SAFETY: standard message pump on the thread that owns the window.
        unsafe {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    smtc.SetIsEnabled(false)?;
    Ok(())
}
