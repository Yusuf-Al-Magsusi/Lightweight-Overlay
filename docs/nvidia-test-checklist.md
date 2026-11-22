# NVIDIA GPU widget: manual test checklist

The GPU widget reads NVIDIA's NVML library (part of the normal NVIDIA driver). The dev machine has an AMD GPU, so the real NVML path has **not** been run against real hardware. It has been tested only against a fake `nvml.dll` that follows `nvml.h`. If you have an NVIDIA card, please run this once.

## Setup
- NVIDIA driver installed (any recent Game Ready or Studio driver).
- Build and run: `cargo run --release -p lwo`.

## Checks
1. **Appears.** A GPU panel shows under Memory with load %, temperature, power in W, and VRAM used / total.
2. **Values are right.** Compare with Task Manager (Performance, GPU) or `nvidia-smi` while idle and while a game or video is running:
   - load % close to `nvidia-smi` utilization.gpu
   - temperature within a degree or two
   - power within a few watts of `nvidia-smi` power.draw (some laptops show `-- W`; that is expected where the driver reports no power)
   - VRAM used / total within about 0.1 GB of `nvidia-smi` memory.used / memory.total (we show GiB, 1024-based)
3. **Updates.** Values change about once per second under load.
4. **Hide and show** with Ctrl+Alt+O: the panel comes back with fresh values, and the app does not grow in memory after many toggles.
5. **Multiple GPUs.** Only GPU 0 is shown. Note which GPU `nvidia-smi` lists as index 0.
6. **No NVIDIA driver** (or on an AMD/Intel PC): the panel falls back to the generic Windows counters and shows only load and VRAM (no temperature or power row). This is verified on the AMD dev machine, including under real load.
7. **NVIDIA wins over the generic path.** On an NVIDIA PC the temperature and power row must be present. If it is missing, NVML failed to load and the generic path took over; tell us.

## Report back
Card model, driver version, whether each check passed, and any wrong value with the `nvidia-smi` value next to it.

## Where NVML is loaded from
`nvml.dll` from the Windows system folder first (current drivers install it there), then `C:\Program Files\NVIDIA Corporation\NVSMI\nvml.dll` (older drivers). Never from the application folder. If your driver puts it somewhere else, the widget stays hidden; tell us the path.
