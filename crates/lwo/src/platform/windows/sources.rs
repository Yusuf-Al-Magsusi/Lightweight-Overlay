//! The lazy source manager. A source exists only if a widget declared it, and
//! it runs only while the overlay is visible: `start` on show, `stop` on hide.
//! While running, polled sources are read once per tick, and only the ones
//! that exist.

use std::sync::Arc;

use lwo_core::widget::{MediaSnapshot, Source, SourceInfo, Stats, Widget};
use windows::Win32::Foundation::HWND;

use super::gpu::GpuSource;
use super::media::MediaSource;
use super::sensors::{read_memory, CpuSampler};

/// What this machine's sources can provide, learned once at startup. A GPU
/// provider is opened, inspected and dropped again (which unloads NVML or
/// closes the counter query), so probing leaves nothing running.
pub fn probe_sources() -> SourceInfo {
    SourceInfo {
        gpu: GpuSource::open().map(|g| g.info()),
    }
}
pub struct Sources {
    media: Option<MediaSource>,
    cpu: Option<CpuSampler>,
    memory: bool,
    gpu_wanted: bool,
    gpu: Option<GpuSource>,
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
    }

    pub fn stop(&mut self) {
        if let Some(m) = &mut self.media {
            m.stop();
        }
        self.gpu = None; // shuts NVML down and unloads it, or closes the counter query
    }

    /// Read every running polled source once.
    pub fn poll(&mut self) -> Stats {
        Stats {
            cpu_load_percent: self.cpu.as_mut().and_then(CpuSampler::sample),
            ram: if self.memory { read_memory() } else { None },
            gpu: self.gpu.as_mut().and_then(GpuSource::read),
        }
    }

    pub fn media_snapshot(&self) -> Option<Arc<MediaSnapshot>> {
        self.media.as_ref().and_then(MediaSource::snapshot)
    }
}
