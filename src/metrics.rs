use crate::store::Store;
use std::{fs, time::Duration};

#[derive(Debug, Clone, Default)]
pub struct HostSample {
    pub cpu_percent: f32,
    pub ram_percent: f32,
    pub load1: f32,
    pub disk_percent: f32,
}

#[derive(Debug, Clone, Copy, Default)]
struct CpuTimes {
    idle: u64,
    total: u64,
}

pub async fn run_collector(store: Store) {
    let mut last_cpu = read_cpu_times();
    loop {
        tokio::time::sleep(Duration::from_secs(15)).await;
        let current_cpu = read_cpu_times();
        let cpu_percent = cpu_percent(last_cpu, current_cpu);
        last_cpu = current_cpu;

        let sample = HostSample {
            cpu_percent,
            ram_percent: read_ram_percent(),
            load1: read_load1(),
            disk_percent: read_disk_percent(),
        };
        store.record_host_metrics(sample);
    }
}

fn read_cpu_times() -> CpuTimes {
    let Ok(content) = fs::read_to_string("/proc/stat") else {
        return CpuTimes::default();
    };
    let Some(line) = content.lines().next() else {
        return CpuTimes::default();
    };
    let numbers: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|part| part.parse().ok())
        .collect();
    if numbers.len() < 4 {
        return CpuTimes::default();
    }
    let idle = numbers.get(3).copied().unwrap_or(0) + numbers.get(4).copied().unwrap_or(0);
    let total = numbers.iter().sum();
    CpuTimes { idle, total }
}

fn cpu_percent(last: CpuTimes, current: CpuTimes) -> f32 {
    let total_delta = current.total.saturating_sub(last.total);
    let idle_delta = current.idle.saturating_sub(last.idle);
    if total_delta == 0 {
        return 0.0;
    }
    let busy = total_delta.saturating_sub(idle_delta);
    ((busy as f32 / total_delta as f32) * 100.0).clamp(0.0, 100.0)
}

fn read_ram_percent() -> f32 {
    let Ok(content) = fs::read_to_string("/proc/meminfo") else {
        return 0.0;
    };
    let mut total = 0.0;
    let mut available = 0.0;
    for line in content.lines() {
        if line.starts_with("MemTotal:") {
            total = meminfo_value(line);
        } else if line.starts_with("MemAvailable:") {
            available = meminfo_value(line);
        }
    }
    if total <= 0.0 {
        return 0.0;
    }
    (((total - available) / total) * 100.0).clamp(0.0, 100.0)
}

fn meminfo_value(line: &str) -> f32 {
    line.split_whitespace()
        .nth(1)
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(0.0)
}

fn read_load1() -> f32 {
    fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|content| content.split_whitespace().next().map(str::to_string))
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(0.0)
}

fn read_disk_percent() -> f32 {
    // v0 intentionally avoids privileged calls and external probes. Disk pressure
    // is reserved for a later release; keeping the field stable helps the UI.
    0.0
}
