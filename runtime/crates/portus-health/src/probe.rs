use portus_protocol::HealthObservation;

pub trait HealthProbeSet: Send + Sync {
    fn collect(&self, now_ms: i64) -> Vec<HealthObservation>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DisabledHealthProbes;

impl HealthProbeSet for DisabledHealthProbes {
    fn collect(&self, _now_ms: i64) -> Vec<HealthObservation> {
        Vec::new()
    }
}
