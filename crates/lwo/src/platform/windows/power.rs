//! CPU package power from Windows' own "Energy Meter" performance counters.
//!
//! No driver of ours, nothing to install, no admin: the values come from a
//! driver that ships with Windows (on the dev PC, Microsoft's processor driver
//! `AmdPPM`). Not every PC provides them; where they are missing the power row
//! simply isn't shown. See docs/sensor-investigation.md.

use windows::Win32::System::Performance::{
    PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhOpenQueryW, PDH_HCOUNTER,
    PDH_HQUERY,
};

use super::gpu::read_items;

const PDH_SUCCESS: u32 = 0;
/// Pause between the baseline and the first real sample: four times the smallest that worked.
const BASELINE_GAP: std::time::Duration = std::time::Duration::from_millis(20);

/// Total package power in watts from the `Energy Meter(*)\Power` instances,
/// which are in milliwatts (checked against the energy counter on the dev PC).
///
/// Only whole-package channels count (names ending in `_pkg`, such as
/// `rapl_package0_pkg`), summed over sockets. Per-core and other channels are
/// parts of a package, so adding them would count the same power twice.
/// `None` if there is no package channel or it reads zero.
pub fn package_watts(items: &[(String, f64)]) -> Option<f32> {
    let milliwatts: f64 = items
        .iter()
        .filter(|(name, _)| name.to_ascii_lowercase().ends_with("_pkg"))
        .map(|(_, value)| value.max(0.0))
        .sum();
    (milliwatts > 0.0).then(|| (milliwatts / 1000.0) as f32)
}

/// An open counter query for CPU power. Dropping it closes the query.
pub struct CpuPower {
    query: PDH_HQUERY,
    counter: PDH_HCOUNTER,
    buffer: Vec<u64>,
}

impl CpuPower {
    /// `None` if this PC has no energy meter with a CPU package channel.
    pub fn open() -> Option<Self> {
        // SAFETY: standard PDH query setup; the query is closed on failure or in Drop.
        let mut meter = unsafe {
            let mut query = PDH_HQUERY::default();
            if PdhOpenQueryW(None, 0, &mut query) != PDH_SUCCESS {
                return None;
            }
            let mut counter = PDH_HCOUNTER::default();
            if PdhAddEnglishCounterW(
                query,
                windows::core::w!("\\Energy Meter(*)\\Power"),
                0,
                &mut counter,
            ) != PDH_SUCCESS
            {
                PdhCloseQuery(query);
                return None;
            }
            Self {
                query,
                counter,
                buffer: Vec::new(),
            }
        };
        // The first sample of this counter never has a value, so take it as a
        // baseline, then wait before the second: taken at once it often reads
        // zero (measured: 11 of 40 checks succeeded with no wait, 40 of 40 from
        // 5 ms up).
        // SAFETY: valid query handle owned by `meter`.
        unsafe { PdhCollectQueryData(meter.query) };
        std::thread::sleep(BASELINE_GAP);
        meter.read()?; // present, and has a package channel with a value
        Some(meter)
    }

    pub fn read(&mut self) -> Option<f32> {
        // SAFETY: valid query handle owned by `self`.
        if unsafe { PdhCollectQueryData(self.query) } != PDH_SUCCESS {
            return None;
        }
        package_watts(&read_items(self.counter, &mut self.buffer)?)
    }
}

impl Drop for CpuPower {
    fn drop(&mut self) {
        // SAFETY: closes the query opened in `open`; not used afterwards.
        unsafe {
            PdhCloseQuery(self.query);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(list: &[(&str, f64)]) -> Vec<(String, f64)> {
        list.iter().map(|(n, v)| (n.to_string(), *v)).collect()
    }

    #[test]
    fn uses_the_package_channel_and_ignores_cores_and_total() {
        // The channels seen on the dev PC.
        let list = items(&[
            ("rapl_package0_core0_core", 1_775.8),
            ("rapl_package0_core1_core", 3_305.9),
            ("rapl_package0_pkg", 37_185.5),
            ("_total", 0.0),
        ]);
        assert!((package_watts(&list).unwrap() - 37.1855).abs() < 0.001);
    }

    #[test]
    fn matches_other_naming_styles_and_sums_sockets() {
        let list = items(&[
            ("RAPL_Package0_PKG", 20_000.0),
            ("RAPL_Package1_PKG", 15_000.0),
        ]);
        assert!((package_watts(&list).unwrap() - 35.0).abs() < 0.001);
    }

    #[test]
    fn no_package_channel_or_a_zero_reading_means_not_available() {
        assert_eq!(package_watts(&[]), None);
        assert_eq!(
            package_watts(&items(&[("rapl_package0_core0_core", 500.0)])),
            None
        );
        assert_eq!(package_watts(&items(&[("rapl_package0_pkg", 0.0)])), None);
        // A battery or board meter must not be mistaken for the CPU.
        assert_eq!(
            package_watts(&items(&[("platform_battery", 9_000.0)])),
            None
        );
    }

    /// Whether the power row appears must not be luck: on a machine that has the
    /// meter, every open has to succeed. (Without the baseline gap most failed.)
    #[test]
    fn opening_the_meter_is_reliable() {
        if (0..5).all(|_| CpuPower::open().is_none()) {
            eprintln!("skipped: no CPU energy meter on this machine");
            return;
        }
        let ok = (0..25).filter(|_| CpuPower::open().is_some()).count();
        assert_eq!(ok, 25, "only {ok} of 25 opens found the CPU power reading");
    }

    /// Against this machine's real counters; skipped where there are none.
    #[test]
    fn live_package_power_is_plausible() {
        let Some(mut meter) = CpuPower::open() else {
            eprintln!("skipped: no CPU energy meter on this machine");
            return;
        };
        std::thread::sleep(std::time::Duration::from_millis(600));
        let watts = meter.read().expect("a second reading");
        println!("live package power: {watts:.1} W");
        assert!((1.0..1000.0).contains(&watts), "implausible: {watts} W");
    }
}
