//! Background sampling of system metrics.
//!
//! sysinfo covers CPU / memory / disk / network (cross-platform); macmon adds
//! GPU utilisation, per-package power and temperature on Apple Silicon. All
//! sampling happens on a dedicated thread so the UI never blocks.

use crossbeam_channel::Sender;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use sysinfo::{Disks, Networks, ProcessesToUpdate, System};

#[derive(Clone, Debug)]
pub struct DiskInfo {
    pub mount: String,
    pub used: u64,
    pub total: u64,
}

impl DiskInfo {
    pub fn frac(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            (self.used as f64 / self.total as f64) as f32
        }
    }
}

#[derive(Clone, Debug)]
pub struct ProcInfo {
    pub name: String,
    pub cpu: f32,   // percent of one core (may exceed 100)
    pub mem: u64,   // resident bytes
}

/// A single point-in-time snapshot handed to the UI thread.
#[derive(Clone, Debug, Default)]
pub struct Sample {
    pub cpu_overall: f32,        // 0..100
    pub per_core: Vec<f32>,      // each 0..100
    pub cpu_freq_mhz: u64,
    pub mem_used: u64,
    pub mem_total: u64,
    pub swap_used: u64,
    pub swap_total: u64,

    pub gpu: Option<f32>,        // 0..100
    pub ecpu: Option<f32>,       // efficiency-cluster active residency 0..100
    pub pcpu: Option<f32>,       // performance-cluster active residency 0..100
    pub cpu_power: Option<f32>,  // Watts
    pub gpu_power: Option<f32>,  // Watts
    pub ane_power: Option<f32>,  // Watts
    pub sys_power: Option<f32>,  // Watts (package/system)
    pub cpu_temp: Option<f32>,   // Celsius
    pub gpu_temp: Option<f32>,   // Celsius

    pub net_rx_bps: f64,         // bytes/sec down
    pub net_tx_bps: f64,         // bytes/sec up

    pub disks: Vec<DiskInfo>,
    pub top_procs: Vec<ProcInfo>,

    pub chip_name: String,
    pub gpu_cores: u8,
}

impl Sample {
    pub fn mem_frac(&self) -> f32 {
        if self.mem_total == 0 {
            0.0
        } else {
            (self.mem_used as f64 / self.mem_total as f64) as f32
        }
    }
    pub fn swap_frac(&self) -> f32 {
        if self.swap_total == 0 {
            0.0
        } else {
            (self.swap_used as f64 / self.swap_total as f64) as f32
        }
    }
}

/// Spawn the sampler thread. It sends a fresh [`Sample`] roughly every
/// `interval` ms and requests an egui repaint so the UI (and tray) refresh.
pub fn spawn(ctx: egui::Context, tx: Sender<Sample>, interval: Arc<AtomicU64>) {
    std::thread::Builder::new()
        .name("pulse-sampler".into())
        .spawn(move || run(ctx, tx, interval))
        .expect("failed to spawn sampler thread");
}

fn run(ctx: egui::Context, tx: Sender<Sample>, interval: Arc<AtomicU64>) {
    let mut sys = System::new();
    let mut networks = Networks::new_with_refreshed_list();
    let mut disks = Disks::new_with_refreshed_list();

    // macmon is Apple-Silicon only; degrade gracefully everywhere else.
    let mut sampler = macmon::Sampler::new().ok();
    let (chip_name, gpu_cores) = match sampler.as_ref() {
        Some(s) => {
            let soc = s.get_soc_info();
            (soc.chip_name.clone(), soc.gpu_cores)
        }
        None => (String::from("Mac"), 0),
    };

    // Prime CPU counters so the first real reading is a valid delta.
    sys.refresh_cpu_all();

    let mut cached_disks = read_disks(&disks);
    let mut cached_procs: Vec<ProcInfo> = Vec::new();
    let mut last_tick = Instant::now();
    let mut cycle: u64 = 0;

    loop {
        let interval_ms = interval.load(Ordering::Relaxed).clamp(500, 10_000);

        // macmon integrates utilisation/power over `interval_ms` and blocks for
        // that long, which conveniently paces the whole loop. Without it we
        // sleep instead.
        let mm = match sampler.as_mut() {
            Some(s) => match s.get_metrics(interval_ms as u32) {
                Ok(m) => Some(m),
                Err(e) => {
                    eprintln!("[pulse] macmon sample failed: {e}");
                    None
                }
            },
            None => {
                std::thread::sleep(std::time::Duration::from_millis(interval_ms));
                None
            }
        };

        let dt = last_tick.elapsed().as_secs_f64().max(0.05);
        last_tick = Instant::now();

        // ---- sysinfo: CPU, memory, network (cheap, every cycle) ----
        sys.refresh_cpu_all();
        sys.refresh_memory();
        networks.refresh(false);

        let cpu_overall = sys.global_cpu_usage();
        let per_core: Vec<f32> = sys.cpus().iter().map(|c| c.cpu_usage()).collect();
        let sysinfo_freq = sys.cpus().iter().map(|c| c.frequency()).max().unwrap_or(0);

        let (mut rx, mut tx_bytes) = (0u64, 0u64);
        for (_name, data) in networks.list() {
            rx += data.received();
            tx_bytes += data.transmitted();
        }
        let net_rx_bps = rx as f64 / dt;
        let net_tx_bps = tx_bytes as f64 / dt;

        // ---- disks: refresh occasionally (they change slowly) ----
        if cycle % 5 == 0 {
            disks.refresh(false);
            cached_disks = read_disks(&disks);
        }

        // ---- processes: moderately expensive, refresh every few cycles ----
        if cycle % 3 == 0 {
            sys.refresh_processes(ProcessesToUpdate::All, true);
            cached_procs = top_processes(&sys, 5);
        }

        // ---- macmon-derived fields ----
        let mut s = Sample {
            cpu_overall,
            per_core,
            cpu_freq_mhz: sysinfo_freq,
            mem_used: sys.used_memory(),
            mem_total: sys.total_memory(),
            swap_used: sys.used_swap(),
            swap_total: sys.total_swap(),
            net_rx_bps,
            net_tx_bps,
            disks: cached_disks.clone(),
            top_procs: cached_procs.clone(),
            chip_name: chip_name.clone(),
            gpu_cores,
            ..Default::default()
        };

        if let Some(m) = mm {
            s.gpu = Some((m.gpu_usage.1 * 100.0).clamp(0.0, 100.0));
            s.ecpu = Some((m.ecpu_usage.1 * 100.0).clamp(0.0, 100.0));
            s.pcpu = Some((m.pcpu_usage.1 * 100.0).clamp(0.0, 100.0));
            s.cpu_power = Some(m.cpu_power);
            s.gpu_power = Some(m.gpu_power);
            s.ane_power = Some(m.ane_power);
            s.sys_power = Some(m.sys_power);
            s.cpu_temp = pos(m.temp.cpu_temp_avg);
            s.gpu_temp = pos(m.temp.gpu_temp_avg);
            if m.pcpu_usage.0 > 0 {
                s.cpu_freq_mhz = m.pcpu_usage.0 as u64;
            }
        }

        // If the receiver is gone the app is shutting down: stop sampling.
        if tx.send(s).is_err() {
            break;
        }
        ctx.request_repaint();
        cycle = cycle.wrapping_add(1);
    }
}

fn pos(v: f32) -> Option<f32> {
    if v > 0.0 { Some(v) } else { None }
}

fn read_disks(disks: &Disks) -> Vec<DiskInfo> {
    let mut out = Vec::new();
    for d in disks.list() {
        let total = d.total_space();
        if total == 0 {
            continue;
        }
        let used = total.saturating_sub(d.available_space());
        let mount = d.mount_point().to_string_lossy().to_string();
        // Skip the noisy read-only system volume and firmlinks; keep real mounts.
        if mount.starts_with("/System/Volumes") && mount != "/System/Volumes/Data" {
            continue;
        }
        out.push(DiskInfo { mount, used, total });
    }
    // De-duplicate volumes that report identical size (APFS containers).
    out.sort_by(|a, b| b.total.cmp(&a.total));
    out.dedup_by(|a, b| a.total == b.total && a.used == b.used);
    out.truncate(4);
    out
}

fn top_processes(sys: &System, n: usize) -> Vec<ProcInfo> {
    let mut procs: Vec<ProcInfo> = sys
        .processes()
        .values()
        .map(|p| ProcInfo {
            name: p.name().to_string_lossy().to_string(),
            cpu: p.cpu_usage(),
            mem: p.memory(),
        })
        .filter(|p| p.cpu > 0.5 || p.mem > 50 * 1024 * 1024)
        .collect();
    procs.sort_by(|a, b| b.cpu.partial_cmp(&a.cpu).unwrap_or(std::cmp::Ordering::Equal));
    procs.truncate(n);
    procs
}
