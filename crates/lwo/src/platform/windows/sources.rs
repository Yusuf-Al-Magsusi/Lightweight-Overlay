//! The lazy source manager. A source exists only if a widget declared it, and
//! it runs only while the overlay is visible: `start` on show, `stop` on hide.
//! While running, polled sources are read once per tick, and only the ones
//! that exist.

use std::sync::Arc;

use lwo_core::widget::{MediaSnapshot, Source, SourceInfo, Stats, Widget};
use windows::Win32::Foundation::HWND;

use super::frames::{self, FrameSource};
use super::gpu::GpuSource;
use super::media::MediaSource;
use super::sensors::{read_memory, CpuSampler};

/// What this machine's sources can provide, learned once at startup. A GPU
/// provider is opened, inspected and dropped again (which unloads NVML or
/// closes the counter query), so probing leaves nothing running.
pub fn probe_sources() -> SourceInfo {
    SourceInfo {
        gpu: GpuSource::open().map(|g| g.info()),
        fps: frames::available(),
    }
}

/// How often (in polls) to ask whether Windows has been dropping frame events.
const LOSS_CHECK_EVERY: u32 = 10;
pub struct Sources {
    media: Option<MediaSource>,
    cpu: Option<CpuSampler>,
    memory: bool,
    gpu_wanted: bool,
    gpu: Option<GpuSource>,
    fps_wanted: bool,
    frames: Option<Box<dyn FrameSource>>,
    polls: u32,
    loss_reported: bool,
}

impl Sources {
    pub fn for_widgets(widgets: &[Box<dyn Widget>]) -> Self {
        let wants = |s: Source| widgets.iter().any(|w| w.sources().contains(&s));
        Self {
            media: wants(Source::Media).then(MediaSource::new),
            cpu: wants(Source::Cpu).then(CpuSampler::new),
            memory: wants(Source::Memory),
            gpu_wanted: wants(Source::Gpu),
            gpu: None,
            fps_wanted: wants(Source::Fps),
            frames: None,
            polls: 0,
            loss_reported: false,
        }
    }

    pub fn start(&mut self, notify: HWND) {
        if let Some(m) = &mut self.media {
            m.start(notify);
        }
        if let Some(c) = &mut self.cpu {
            c.reset();
        }
        if self.gpu_wanted && self.gpu.is_none() {
            self.gpu = GpuSource::open();
        }
        if self.fps_wanted && self.frames.is_none() {
            self.frames = frames::start();
        }
    }

    pub fn stop(&mut self) {
        if let Some(m) = &mut self.media {
            m.stop();
        }
        self.gpu = None; // shuts NVML down and unloads it, or closes the counter query
        self.frames = None; // stops the Windows tracing session
    }

    /// Read every running polled source once.
    pub fn poll(&mut self) -> Stats {
        Stats {
            cpu_load_percent: self.cpu.as_mut().and_then(CpuSampler::sample),
            ram: if self.memory { read_memory() } else { None },
            gpu: self.gpu.as_mut().and_then(GpuSource::read),
            fps: self
                .frames
                .as_mut()
                .and_then(|f| frames::foreground_pid().and_then(|pid| f.fps(pid))),
        }
    }

    /// A message to show once if Windows has been dropping frame events, which
    /// makes the FPS read low. Checked every few polls.
    pub fn take_loss_warning(&mut self) -> Option<String> {
        self.polls = self.polls.wrapping_add(1);
        if self.loss_reported || !self.polls.is_multiple_of(LOSS_CHECK_EVERY) {
            return None;
        }
        let lost = self.frames.as_mut()?.events_lost();
        if lost == 0 {
            return None;
        }
        self.loss_reported = true;
        Some(format!(
            "Windows dropped {lost} frame events, so the FPS reading may be too low."
        ))
    }

    pub fn media_snapshot(&self) -> Option<Arc<MediaSnapshot>> {
        self.media.as_ref().and_then(MediaSource::snapshot)
    }
}
