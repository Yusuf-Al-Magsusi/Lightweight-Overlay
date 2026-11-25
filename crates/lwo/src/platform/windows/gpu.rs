//! GPU source: a provider chain.
//!
//! 1. NVML (NVIDIA): load, temperature, power, VRAM. See `sensors.rs`.
//! 2. Windows performance counters, for any vendor: load and VRAM only. These
//!    are the same counters Task Manager uses. User-mode and driver-free.
//!
//! The provider is chosen once at startup (so the panel's shape is fixed) and
//! opened again each time the overlay is shown.

use std::collections::HashMap;

use lwo_core::widget::{GpuInfo, GpuStats};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIAdapter1, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE,
};
use windows::Win32::System::Performance::{
    PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW,
    PdhOpenQueryW, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY,
};

use super::sensors::Nvml;

/// Microsoft's software renderer ("Microsoft Basic Render Driver").
const VENDOR_MICROSOFT: u32 = 0x1414;
const PDH_SUCCESS: u32 = 0;
const PDH_MORE_DATA: u32 = 0x8000_07D2;
/// `PDH_FMT_COUNTERVALUE::CStatus` values that carry usable data.
const PDH_CSTATUS_VALID_DATA: u32 = 0;
const PDH_CSTATUS_NEW_DATA: u32 = 1;

pub enum GpuSource {
    Nvml(Nvml),
    Counters(Counters),
}

impl GpuSource {
    /// Best available provider, or `None` if this machine has no usable GPU source.
    pub fn open() -> Option<Self> {
        Nvml::open()
            .map(Self::Nvml)
            .or_else(|| Counters::open().map(Self::Counters))
    }

    /// Which optional fields this provider can fill in.
    pub fn info(&self) -> GpuInfo {
        match self {
            Self::Nvml(_) => GpuInfo {
                has_temperature: true,
                has_power: true,
            },
            Self::Counters(_) => GpuInfo::default(),
        }
    }

    pub fn read(&mut self) -> Option<GpuStats> {
        match self {
            Self::Nvml(n) => n.read(),
            Self::Counters(c) => c.read(),
        }
    }
}

// ------------------------------------------------------- pure helpers

/// An adapter as reported by DXGI.
#[derive(Clone, Debug, PartialEq)]
pub struct AdapterInfo {
    pub name: String,
    pub luid_high: i32,
    pub luid_low: u32,
    pub dedicated_vram: u64,
    pub software: bool,
    pub vendor_id: u32,
}

/// The adapter's identifier as it appears in counter instance names, for
/// example `luid_0x00000001_0x87386649` (compared case-insensitively).
pub fn luid_key(high: i32, low: u32) -> String {
    format!("luid_0x{:08x}_0x{:08x}", high as u32, low)
}

/// Real (non-software) adapters, most dedicated VRAM first, so a discrete card
/// ranks above integrated graphics. Ties keep DXGI's order.
pub fn rank_adapters(adapters: &[AdapterInfo]) -> Vec<&AdapterInfo> {
    let mut real: Vec<&AdapterInfo> = adapters
        .iter()
        .filter(|a| !a.software && a.vendor_id != VENDOR_MICROSOFT)
        .collect();
    real.sort_by_key(|a| std::cmp::Reverse(a.dedicated_vram));
    real
}

/// The adapter to monitor: the best-ranked one that actually shows up in the
/// counters. DXGI can list one physical GPU more than once (this was seen with
/// an AMD card listed twice under different LUIDs) while the counters know
/// only one of them, so ranking by VRAM alone can pick one with no data.
pub fn pick_adapter<'a>(
    adapters: &'a [AdapterInfo],
    counter_instances: &[(String, f64)],
) -> Option<&'a AdapterInfo> {
    rank_adapters(adapters).into_iter().find(|a| {
        let key = luid_key(a.luid_high, a.luid_low);
        counter_instances.iter().any(|(n, _)| contains_ci(n, &key))
    })
}

fn contains_ci(haystack: &str, needle_lower: &str) -> bool {
    haystack.to_ascii_lowercase().contains(needle_lower)
}

/// GPU load for one adapter, from `GPU Engine\Utilization Percentage` items
/// (`pid_N_luid_..._phys_0_eng_N_engtype_X`). Like Task Manager: sum the
/// instances of each engine type (3D, Copy, Video Decode...) and take the
/// busiest type. Clamped to 0..=100.
pub fn engine_load(items: &[(String, f64)], luid_lower: &str) -> Option<f32> {
    let mut by_type: HashMap<&str, f64> = HashMap::new();
    for (name, value) in items {
        if !contains_ci(name, luid_lower) {
            continue;
        }
        let Some(idx) = name.rfind("engtype_") else {
            continue;
        };
        *by_type
            .entry(&name[idx + "engtype_".len()..])
            .or_insert(0.0) += value;
    }
    by_type
        .values()
        .copied()
        .reduce(f64::max)
        .map(|v| v.clamp(0.0, 100.0) as f32)
}

/// Dedicated VRAM in use for one adapter, from `GPU Adapter Memory\Dedicated Usage`
/// items (`luid_..._phys_0`).
pub fn dedicated_usage(items: &[(String, f64)], luid_lower: &str) -> Option<u64> {
    items
        .iter()
        .find(|(name, _)| contains_ci(name, luid_lower))
        .map(|(_, v)| v.max(0.0) as u64)
}

// ---------------------------------------------------------------- DXGI

fn enumerate_adapters() -> Vec<AdapterInfo> {
    let mut out = Vec::new();
    // SAFETY: plain DXGI factory creation and adapter enumeration.
    unsafe {
        let Ok(factory) = CreateDXGIFactory1::<IDXGIFactory1>() else {
            return out;
        };
        let mut i = 0;
        while let Ok(adapter) = factory.EnumAdapters1(i) {
            i += 1;
            let adapter: IDXGIAdapter1 = adapter;
            let Ok(d) = adapter.GetDesc1() else {
                continue;
            };
            let len = d.Description.iter().position(|&c| c == 0).unwrap_or(0);
            out.push(AdapterInfo {
                name: String::from_utf16_lossy(&d.Description[..len]),
                luid_high: d.AdapterLuid.HighPart,
                luid_low: d.AdapterLuid.LowPart,
                dedicated_vram: d.DedicatedVideoMemory as u64,
                software: d.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0,
                vendor_id: d.VendorId,
            });
        }
    }
    out
}

// ----------------------------------------------------------------- PDH

/// Vendor-neutral GPU load and VRAM from Windows performance counters.
pub struct Counters {
    query: PDH_HQUERY,
    engine: PDH_HCOUNTER,
    memory: PDH_HCOUNTER,
    luid_lower: String,
    total_vram: u64,
    engine_buf: Vec<u64>,
    memory_buf: Vec<u64>,
}

impl Counters {
    pub fn open() -> Option<Self> {
        let adapters = enumerate_adapters();
        if rank_adapters(&adapters).is_empty() {
            return None;
        }

        // SAFETY: standard PDH query setup; every handle is closed on failure or in Drop.
        let mut counters = unsafe {
            let mut query = PDH_HQUERY::default();
            if PdhOpenQueryW(None, 0, &mut query) != PDH_SUCCESS {
                return None;
            }
            let mut engine = PDH_HCOUNTER::default();
            let mut memory = PDH_HCOUNTER::default();
            let ok = PdhAddEnglishCounterW(
                query,
                windows::core::w!("\\GPU Engine(*)\\Utilization Percentage"),
                0,
                &mut engine,
            ) == PDH_SUCCESS
                && PdhAddEnglishCounterW(
                    query,
                    windows::core::w!("\\GPU Adapter Memory(*)\\Dedicated Usage"),
                    0,
                    &mut memory,
                ) == PDH_SUCCESS;
            if !ok {
                PdhCloseQuery(query);
                return None;
            }
            // Baseline sample: rate counters need two samples before they have a value.
            PdhCollectQueryData(query);
            Self {
                query,
                engine,
                memory,
                luid_lower: String::new(),
                total_vram: 0,
                engine_buf: Vec::new(),
                memory_buf: Vec::new(),
            }
        };

        // Memory is a plain gauge, so it already has values after one sample:
        // use it to see which adapters the counters really know about.
        let instances = read_items(counters.memory, &mut counters.memory_buf)?;
        let a = pick_adapter(&adapters, &instances)?;
        counters.luid_lower = luid_key(a.luid_high, a.luid_low);
        counters.total_vram = a.dedicated_vram;
        Some(counters) // on any `?` above, Drop closes the query
    }

    /// `None` until a second sample exists, or if the counters stop answering.
    pub fn read(&mut self) -> Option<GpuStats> {
        // SAFETY: valid query handle owned by `self`.
        if unsafe { PdhCollectQueryData(self.query) } != PDH_SUCCESS {
            return None;
        }
        let engine = read_items(self.engine, &mut self.engine_buf)?;
        let memory = read_items(self.memory, &mut self.memory_buf)?;
        let load = engine_load(&engine, &self.luid_lower)?;
        let used = dedicated_usage(&memory, &self.luid_lower)?;
        Some(GpuStats {
            load_percent: load.round() as u32,
            temperature_c: None,
            power_watts: None,
            vram_used_bytes: used,
            vram_total_bytes: self.total_vram.max(used),
        })
    }
}

impl Drop for Counters {
    fn drop(&mut self) {
        // SAFETY: closes the query opened in `open`; not used afterwards.
        unsafe {
            PdhCloseQuery(self.query);
        }
    }
}

/// Read every instance of a wildcard counter as `(instance name, value)`.
pub(super) fn read_items(counter: PDH_HCOUNTER, buf: &mut Vec<u64>) -> Option<Vec<(String, f64)>> {
    let mut bytes: u32 = (buf.len() * 8) as u32;
    let mut count: u32 = 0;
    // The first call may report the needed size; retry once with a bigger buffer.
    for _ in 0..2 {
        let ptr = if buf.is_empty() {
            None
        } else {
            Some(buf.as_mut_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>())
        };
        // SAFETY: `buf` is at least `bytes` long and 8-byte aligned (Vec<u64>);
        // PDH writes items and their name strings into it.
        let status = unsafe {
            PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &mut bytes, &mut count, ptr)
        };
        match status {
            PDH_SUCCESS => {
                let items = buf.as_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>();
                let mut out = Vec::with_capacity(count as usize);
                for i in 0..count as usize {
                    // SAFETY: PDH filled `count` items at the start of `buf`.
                    let item = unsafe { &*items.add(i) };
                    let status = item.FmtValue.CStatus;
                    if status != PDH_CSTATUS_VALID_DATA && status != PDH_CSTATUS_NEW_DATA {
                        continue;
                    }
                    // SAFETY: the name is a NUL-terminated string inside `buf`;
                    // we requested PDH_FMT_DOUBLE so `doubleValue` is the active field.
                    let (name, value) = unsafe {
                        (
                            item.szName.to_string().unwrap_or_default(),
                            item.FmtValue.Anonymous.doubleValue,
                        )
                    };
                    out.push((name, value));
                }
                return Some(out);
            }
            PDH_MORE_DATA => buf.resize((bytes as usize).div_ceil(8) + 8, 0),
            _ => return None,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter(name: &str, low: u32, vram_gib: u64, software: bool, vendor: u32) -> AdapterInfo {
        AdapterInfo {
            name: name.into(),
            luid_high: 1,
            luid_low: low,
            dedicated_vram: vram_gib * 1024 * 1024 * 1024,
            software,
            vendor_id: vendor,
        }
    }

    #[test]
    fn luid_key_matches_counter_naming() {
        assert_eq!(luid_key(1, 0x87386649), "luid_0x00000001_0x87386649");
        assert_eq!(luid_key(0, 0x18912), "luid_0x00000000_0x00018912");
    }

    fn instances(luids: &[&str]) -> Vec<(String, f64)> {
        luids.iter().map(|l| (format!("{l}_phys_0"), 0.0)).collect()
    }

    #[test]
    fn ranks_discrete_above_integrated_and_drops_software_adapters() {
        let list = [
            adapter("Integrated", 1, 1, false, 0x1002),
            adapter("Discrete", 2, 16, false, 0x1002),
            adapter("Basic Render", 3, 32, true, 0x1414),
        ];
        let names: Vec<_> = rank_adapters(&list)
            .iter()
            .map(|a| a.name.clone())
            .collect();
        assert_eq!(names, ["Discrete", "Integrated"]);
    }

    #[test]
    fn microsoft_vendor_is_ignored_even_without_the_software_flag() {
        assert!(rank_adapters(&[adapter("Basic Render", 3, 8, false, 0x1414)]).is_empty());
    }

    #[test]
    fn picks_the_best_adapter_that_the_counters_know() {
        let list = [
            adapter("Integrated", 1, 1, false, 0x1002),
            adapter("Discrete", 2, 16, false, 0x1002),
        ];
        let both = instances(&[&luid_key(1, 1), &luid_key(1, 2)]);
        assert_eq!(pick_adapter(&list, &both).unwrap().name, "Discrete");
        // Counters only know the integrated one: fall back to it.
        let only_igpu = instances(&[&luid_key(1, 1)]);
        assert_eq!(pick_adapter(&list, &only_igpu).unwrap().name, "Integrated");
    }

    #[test]
    fn a_gpu_listed_twice_by_dxgi_resolves_to_the_twin_with_counters() {
        // Seen on a real AMD RX 9070 XT: same card, two LUIDs, equal VRAM,
        // and the counters only know the second one.
        let mut first = adapter("RX 9070 XT", 100, 16, false, 0x1002);
        first.luid_high = 0;
        let second = adapter("RX 9070 XT", 200, 16, false, 0x1002);
        let list = [first, second];
        let counters = instances(&[&luid_key(1, 200)]);
        let picked = pick_adapter(&list, &counters).unwrap();
        assert_eq!((picked.luid_high, picked.luid_low), (1, 200));
    }

    #[test]
    fn no_adapters_or_no_matching_counters_means_no_provider() {
        assert!(pick_adapter(&[], &[]).is_none());
        let list = [adapter("Discrete", 2, 16, false, 0x1002)];
        assert!(pick_adapter(&list, &instances(&[&luid_key(1, 99)])).is_none());
    }

    fn engine(pid: u32, luid: &str, eng: u32, ty: &str) -> String {
        format!("pid_{pid}_{luid}_phys_0_eng_{eng}_engtype_{ty}")
    }

    #[test]
    fn engine_load_sums_per_type_and_takes_the_busiest_type() {
        let mine = "luid_0x00000001_0x87386649";
        let other = "luid_0x00000001_0x87205d13";
        let items = vec![
            (engine(10, mine, 0, "3D"), 30.0),
            (engine(11, mine, 0, "3D"), 25.0), // two processes on 3D: 55
            (engine(10, mine, 1, "Copy"), 40.0),
            (engine(12, other, 0, "3D"), 99.0), // different adapter: ignored
        ];
        assert_eq!(engine_load(&items, mine), Some(55.0));
    }

    #[test]
    fn engine_load_is_case_insensitive_and_clamped() {
        let items = vec![(engine(1, "LUID_0X00000001_0X87386649", 0, "3D"), 180.0)];
        assert_eq!(
            engine_load(&items, "luid_0x00000001_0x87386649"),
            Some(100.0)
        );
    }

    #[test]
    fn engine_load_is_none_when_the_adapter_has_no_instances() {
        let items = vec![(engine(1, "luid_0x00000001_0x11111111", 0, "3D"), 5.0)];
        assert_eq!(engine_load(&items, "luid_0x00000001_0x87386649"), None);
        assert_eq!(engine_load(&[], "luid_0x00000001_0x87386649"), None);
    }

    #[test]
    fn dedicated_usage_finds_the_matching_adapter() {
        let items = vec![
            ("luid_0x00000001_0x87205d13_phys_0".to_string(), 2.0e6),
            ("luid_0x00000001_0x87386649_phys_0".to_string(), 1.5e9),
        ];
        assert_eq!(
            dedicated_usage(&items, "luid_0x00000001_0x87386649"),
            Some(1_500_000_000)
        );
        assert_eq!(dedicated_usage(&items, "luid_0x00000009_0x1"), None);
    }

    /// Runs against this machine's real counters. Skipped where there is no
    /// usable GPU (for example a CI runner with only the software renderer).
    #[test]
    fn live_counters_give_sane_values() {
        let Some(mut c) = Counters::open() else {
            eprintln!("skipped: no usable GPU or counters on this machine");
            return;
        };
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let g = c.read().unwrap_or_else(|| {
            let adapters = enumerate_adapters();
            let engine = unsafe { PdhCollectQueryData(c.query) }; // keep the baseline fresh
            let items = read_items(c.engine, &mut c.engine_buf);
            let mem = read_items(c.memory, &mut c.memory_buf);
            panic!(
                "no data. wanted {:?}\nadapters: {:?}\ncollect={engine:#x}\nengine items: {:?}\nmemory items: {:?}",
                c.luid_lower,
                adapters,
                items.as_ref().map(|v| v.iter().take(4).collect::<Vec<_>>()),
                mem
            )
        });
        assert!(g.load_percent <= 100, "load {}", g.load_percent);
        assert!(g.vram_total_bytes >= g.vram_used_bytes);
        assert!(g.temperature_c.is_none() && g.power_watts.is_none());
    }
}
