use windows::Win32::Foundation::FILETIME;
use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

/// CPU do PRÓPRIO agente (meta: média < 1%).
pub struct CpuMeter {
    last_cpu_100ns: u64,
    last_wall: std::time::Instant,
    ncpu: f32,
}

fn ft(f: FILETIME) -> u64 {
    (u64::from(f.dwHighDateTime) << 32) | u64::from(f.dwLowDateTime)
}

fn cpu_100ns() -> u64 {
    let (mut c, mut e, mut k, mut u) = Default::default();
    unsafe {
        if GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u).is_err() {
            return 0;
        }
    }
    ft(k) + ft(u)
}

impl CpuMeter {
    pub fn new() -> Self {
        Self {
            last_cpu_100ns: cpu_100ns(),
            last_wall: std::time::Instant::now(),
            ncpu: std::thread::available_parallelism()
                .map(|n| n.get() as f32)
                .unwrap_or(1.0),
        }
    }

    pub fn sample(&mut self) -> f32 {
        let now_cpu = cpu_100ns();
        let wall = self.last_wall.elapsed().as_secs_f32();
        let used = now_cpu.saturating_sub(self.last_cpu_100ns) as f32 / 1e7;
        self.last_cpu_100ns = now_cpu;
        self.last_wall = std::time::Instant::now();
        if wall <= 0.0 {
            0.0
        } else {
            used / wall / self.ncpu * 100.0
        }
    }
}

impl Default for CpuMeter {
    fn default() -> Self {
        Self::new()
    }
}

pub fn mem_mb() -> f32 {
    let mut pmc = PROCESS_MEMORY_COUNTERS::default();
    unsafe {
        if GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut pmc,
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        )
        .is_err()
        {
            return 0.0;
        }
    }
    pmc.WorkingSetSize as f32 / (1024.0 * 1024.0)
}
