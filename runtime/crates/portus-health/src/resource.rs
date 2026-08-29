use portus_protocol::{
    HealthComponentType, HealthObservation, HealthReasonCode, HealthState, RecoveryDisposition,
};
use std::collections::BTreeMap;

const GIB: u64 = 1024 * 1024 * 1024;
const MIB: u64 = 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageSample {
    pub component_ref: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub writable: bool,
    pub observed_at_ms: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemorySample {
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub observed_at_ms: i64,
}

#[must_use]
pub fn classify_storage(sample: &StorageSample) -> HealthObservation {
    let percent = sample
        .available_bytes
        .saturating_mul(100)
        .checked_div(sample.total_bytes)
        .unwrap_or(0);
    let (state, reason, summary) = if !sample.writable {
        (
            HealthState::Unavailable,
            HealthReasonCode::ResourceUnavailable,
            "required filesystem is not writable",
        )
    } else if percent < 5 || sample.available_bytes < 512 * MIB {
        (
            HealthState::Degraded,
            HealthReasonCode::ResourceCritical,
            "filesystem free space is critically low",
        )
    } else if percent < 10 || sample.available_bytes < 2 * GIB {
        (
            HealthState::Degraded,
            HealthReasonCode::ResourceLow,
            "filesystem free space is low",
        )
    } else {
        (
            HealthState::Healthy,
            HealthReasonCode::Ready,
            "filesystem capacity is within the first-ISO operating threshold",
        )
    };
    let mut details = BTreeMap::new();
    details.insert("total_bytes".into(), sample.total_bytes.to_string());
    details.insert("available_bytes".into(), sample.available_bytes.to_string());
    details.insert("available_percent".into(), percent.to_string());
    HealthObservation {
        component_ref: sample.component_ref.clone(),
        component_type: HealthComponentType::Storage,
        owner: None,

        health_state: state,
        reason_code: reason,
        summary: summary.into(),
        source: "native-storage".into(),
        observed_at_ms: sample.observed_at_ms,
        source_generation: None,
        last_healthy_at_ms: (state == HealthState::Healthy).then_some(sample.observed_at_ms),
        recovery_disposition: RecoveryDisposition::Observe,
        recovery_attempt_count: 0,
        safe_details: details,
    }
}

#[must_use]
pub fn classify_memory(sample: &MemorySample) -> HealthObservation {
    let percent = sample
        .available_bytes
        .saturating_mul(100)
        .checked_div(sample.total_bytes)
        .unwrap_or(0);
    let (state, reason, summary) = if percent < 2 || sample.available_bytes < 256 * MIB {
        (
            HealthState::Degraded,
            HealthReasonCode::ResourceCritical,
            "available memory is critically low for reliable control-plane operation",
        )
    } else if percent < 5 || sample.available_bytes < 512 * MIB {
        (
            HealthState::Degraded,
            HealthReasonCode::ResourceLow,
            "available memory is low for reliable control-plane operation",
        )
    } else {
        (
            HealthState::Healthy,
            HealthReasonCode::Ready,
            "available memory is within the first-ISO operating threshold",
        )
    };
    let mut details = BTreeMap::new();
    details.insert("total_bytes".into(), sample.total_bytes.to_string());
    details.insert("available_bytes".into(), sample.available_bytes.to_string());
    details.insert("available_percent".into(), percent.to_string());
    HealthObservation {
        component_ref: "memory:system".into(),
        component_type: HealthComponentType::Memory,
        owner: None,

        health_state: state,
        reason_code: reason,
        summary: summary.into(),
        source: "native-memory".into(),
        observed_at_ms: sample.observed_at_ms,
        source_generation: None,
        last_healthy_at_ms: (state == HealthState::Healthy).then_some(sample.observed_at_ms),
        recovery_disposition: RecoveryDisposition::Observe,
        recovery_attempt_count: 0,
        safe_details: details,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_thresholds_follow_locked_or_rules_without_cleanup_side_effects() {
        let healthy = classify_storage(&StorageSample {
            component_ref: "storage:/var/lib/portus".into(),
            total_bytes: 100 * GIB,
            available_bytes: 20 * GIB,
            writable: true,
            observed_at_ms: 1,
        });
        assert_eq!(healthy.health_state, HealthState::Healthy);
        let low = classify_storage(&StorageSample {
            component_ref: "storage:/workspace".into(),
            total_bytes: 100 * GIB,
            available_bytes: 7 * GIB,
            writable: true,
            observed_at_ms: 1,
        });
        assert_eq!(low.reason_code, HealthReasonCode::ResourceLow);
        let critical = classify_storage(&StorageSample {
            component_ref: "storage:/var/log/portus".into(),
            total_bytes: 100 * GIB,
            available_bytes: 400 * MIB,
            writable: true,
            observed_at_ms: 1,
        });
        assert_eq!(critical.reason_code, HealthReasonCode::ResourceCritical);
    }

    #[test]
    fn memory_pressure_is_bounded_observation_not_process_killing_policy() {
        let low = classify_memory(&MemorySample {
            total_bytes: 16 * GIB,
            available_bytes: 400 * MIB,
            observed_at_ms: 5,
        });
        assert_eq!(low.health_state, HealthState::Degraded);
        assert_eq!(low.recovery_disposition, RecoveryDisposition::Observe);
    }
}
