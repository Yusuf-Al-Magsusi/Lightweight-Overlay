//! Where frame rates come from.
//!
//! **This is the seam for changing how FPS is measured.** The rest of the app
//! only uses [`available`], [`start`], [`cleanup_leftover`] and the
//! [`FrameSource`] trait. Today `start` returns [`etw::EtwFrames`], which reads
//! Windows' event stream inside the overlay itself and never asks for
//! elevation ("Option A", docs/decisions.md D24).
//!
//! To switch to an elevated helper later ("Option B"): add a `helper.rs` whose
//! type implements `FrameSource` by receiving rates from the helper over a local
//! pipe, make `available` and `start` choose it, and move `etw.rs` into the
//! helper. The frame counting is already OS-independent (`lwo_core::fps`), and
//! nothing outside this folder changes.

mod etw;

use windows::Win32::Security::{
    CheckTokenMembership, CreateWellKnownSid, WinBuiltinAdministratorsSid,
    WinBuiltinPerfLoggingUsersSid, PSID, SECURITY_MAX_SID_SIZE, WELL_KNOWN_SID_TYPE,
};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

/// A running supply of frame rates. Dropping it releases everything.
pub trait FrameSource {
    /// Frames per second the process drew recently, or `None` if it isn't drawing.
    fn fps(&mut self, pid: u32) -> Option<f32>;
    /// Frame events Windows has dropped since the source started. Above zero
    /// means rates may read low.
    fn events_lost(&mut self) -> u32;
}

fn token_has(kind: WELL_KNOWN_SID_TYPE) -> bool {
    let mut sid = [0u8; SECURITY_MAX_SID_SIZE as usize];
    let mut size = sid.len() as u32;
    let psid = PSID(sid.as_mut_ptr().cast());
    // SAFETY: `sid` is large enough for any SID; `None` means the current token.
    unsafe {
        if CreateWellKnownSid(kind, None, Some(psid), &mut size).is_err() {
            return false;
        }
        let mut member = windows::core::BOOL(0);
        // Only counts groups that are enabled: a non-elevated administrator's
        // "deny only" Administrators group does not count, which is correct.
        CheckTokenMembership(None, psid, &mut member).is_ok() && member.as_bool()
    }
}

/// Whether this process is allowed to read frame events: in "Performance Log
/// Users", or running elevated. A cheap look at our own token; nothing starts.
pub fn available() -> bool {
    // Debug builds only: pretend to be an account without the privilege, so that
    // path can be tested on a machine that has it. Release builds never read this.
    #[cfg(debug_assertions)]
    if std::env::var_os("LWO_FPS_DENIED").is_some() {
        return false;
    }
    token_has(WinBuiltinPerfLoggingUsersSid) || token_has(WinBuiltinAdministratorsSid)
}

/// Start reading frame rates. `None` if it can't be done (no privilege, or
/// Windows has no free tracing session).
pub fn start() -> Option<Box<dyn FrameSource>> {
    etw::EtwFrames::start().map(|s| Box::new(s) as Box<dyn FrameSource>)
}

/// Stop anything an earlier, killed run left behind. Safe to call at any time
/// before `start`.
pub fn cleanup_leftover() {
    etw::stop_leftover_session();
}

/// The process that owns the window in front, if it isn't us. Only asks Windows
/// about the window: **no handle to that process is ever opened.**
pub fn foreground_pid() -> Option<u32> {
    let mut pid = 0u32;
    // SAFETY: plain queries; a null window just leaves `pid` at zero.
    unsafe {
        GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut pid));
        (pid != 0 && pid != GetCurrentProcessId()).then_some(pid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    #[test]
    fn privilege_check_runs_without_panicking() {
        // The answer depends on the account; the call itself must work.
        let _ = available();
    }

    fn present_rate_exe() -> Option<std::path::PathBuf> {
        let exe = std::env::current_exe().ok()?;
        exe.ancestors()
            .skip(1)
            .take(8)
            .flat_map(|dir| {
                [
                    dir.join("examples").join("present_rate.exe"),
                    dir.join("present_rate.exe"),
                ]
            })
            .find(|p| p.exists())
    }

    /// Accuracy against a program that presents at a known rate. Needs the
    /// privilege and a GPU, so it only runs when asked:
    /// `cargo build --release --example present_rate`
    /// `cargo test --release -p lwo accuracy -- --ignored --nocapture`
    #[test]
    #[ignore = "needs Performance Log Users (or admin), a GPU and the present_rate example"]
    fn accuracy_against_a_known_present_rate() {
        assert!(available(), "this account may not read frame events");
        let exe = present_rate_exe().expect("build the present_rate example first");
        let mut source = start().expect("start the frame source");

        for target in [30u32, 100, 144] {
            let child = Command::new(&exe)
                .args([target.to_string(), "14".to_string()])
                .stdout(Stdio::piped())
                .spawn()
                .expect("run present_rate");
            let pid = child.id();
            std::thread::sleep(Duration::from_secs(5)); // warm up: delivery delay plus the window
            let mut readings = Vec::new();
            let until = Instant::now() + Duration::from_secs(6);
            while Instant::now() < until {
                if let Some(f) = source.fps(pid) {
                    readings.push(f);
                }
                std::thread::sleep(Duration::from_millis(1000));
            }
            let output = child.wait_with_output().expect("present_rate output");
            let truth = String::from_utf8_lossy(&output.stdout);
            let actual: f32 = truth
                .lines()
                .find_map(|l| l.strip_prefix("actual_fps="))
                .and_then(|v| v.trim().parse().ok())
                .expect("present_rate must report its real rate");
            let mean = readings.iter().sum::<f32>() / readings.len().max(1) as f32;
            println!(
                "target {target}: program really presented {actual:.1} fps; source read mean {mean:.1} from {readings:?}; events lost {}",
                source.events_lost()
            );
            assert!(readings.len() >= 4, "too few readings");
            assert!(
                (mean - actual).abs() <= actual * 0.03 + 1.0,
                "mean {mean} is more than 3% from the real {actual}"
            );
        }
        // A program that has exited reads as nothing, not as its old rate.
        assert_eq!(source.events_lost(), 0);
    }
}
