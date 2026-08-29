use crate::{HealthProbeSet, MemorySample, StorageSample, classify_memory, classify_storage};
use nix::sys::statvfs::statvfs;
use portus_protocol::{
    HealthComponentType, HealthObservation, HealthReasonCode, HealthState, RecoveryDisposition,
};
use std::{fs, path::PathBuf};

#[derive(Clone, Debug)]
pub struct LinuxHealthProbes {
    storage_paths: Vec<PathBuf>,
}

impl Default for LinuxHealthProbes {
    fn default() -> Self {
        Self {
            storage_paths: vec![
                PathBuf::from("/var/lib/portus"),
                PathBuf::from("/workspace"),
                PathBuf::from("/var/log/portus"),
            ],
        }
    }
}

impl HealthProbeSet for LinuxHealthProbes {
    fn collect(&self, now_ms: i64) -> Vec<HealthObservation> {
        let mut observations = self
            .storage_paths
            .iter()
            .map(|path| storage_observation(path, now_ms))
            .collect::<Vec<_>>();
        observations.push(memory_observation(now_ms));
        observations
    }
}

fn storage_observation(path: &PathBuf, now_ms: i64) -> HealthObservation {
    match statvfs(path) {
        Ok(stat) => classify_storage(&StorageSample {
            component_ref: format!("storage:{}", path.display()),
            total_bytes: stat.blocks().saturating_mul(stat.fragment_size()),
            available_bytes: stat.blocks_available().saturating_mul(stat.fragment_size()),
            writable: !stat.flags().contains(nix::sys::statvfs::FsFlags::ST_RDONLY),
            observed_at_ms: now_ms,
        }),
        Err(_) => HealthObservation {
            component_ref: format!("storage:{}", path.display()),
            component_type: HealthComponentType::Storage,
            owner: None,

            health_state: HealthState::Unknown,
            reason_code: HealthReasonCode::StatusUnavailable,
            summary: "filesystem condition could not be established".into(),
            source: "native-storage".into(),
            observed_at_ms: now_ms,
            source_generation: None,
            last_healthy_at_ms: None,
            recovery_disposition: RecoveryDisposition::Observe,
            recovery_attempt_count: 0,
            safe_details: Default::default(),
        },
    }
}

fn memory_observation(now_ms: i64) -> HealthObservation {
    match fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|contents| parse_meminfo(&contents))
    {
        Some(sample) => classify_memory(&MemorySample {
            total_bytes: sample.0,
            available_bytes: sample.1,
            observed_at_ms: now_ms,
        }),
        None => HealthObservation {
            component_ref: "memory:system".into(),
            component_type: HealthComponentType::Memory,
            owner: None,

            health_state: HealthState::Unknown,
            reason_code: HealthReasonCode::StatusUnavailable,
            summary: "memory availability could not be established".into(),
            source: "native-memory".into(),
            observed_at_ms: now_ms,
            source_generation: None,
            last_healthy_at_ms: None,
            recovery_disposition: RecoveryDisposition::Observe,
            recovery_attempt_count: 0,
            safe_details: Default::default(),
        },
    }
}

fn parse_meminfo(contents: &str) -> Option<(u64, u64)> {
    let mut total_kib = None;
    let mut available_kib = None;
    for line in contents.lines() {
        let (name, rest) = line.split_once(':')?;
        if name == "MemTotal" || name == "MemAvailable" {
            let value = rest.split_whitespace().next()?.parse::<u64>().ok()?;
            match name {
                "MemTotal" => total_kib = Some(value),
                "MemAvailable" => available_kib = Some(value),
                _ => {}
            }
        }
    }
    Some((
        total_kib?.saturating_mul(1024),
        available_kib?.saturating_mul(1024),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meminfo_parser_uses_only_total_and_available() {
        let sample = "MemTotal:       16384000 kB\nMemFree: 1 kB\nMemAvailable:    4096000 kB\n";
        assert_eq!(
            parse_meminfo(sample),
            Some((16_384_000 * 1024, 4_096_000 * 1024))
        );
    }
}
