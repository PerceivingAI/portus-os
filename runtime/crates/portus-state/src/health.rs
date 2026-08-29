use crate::{
    NewSignificantEvent, PortusState, StateError, StateResult,
    safety::{secret_like_key, secret_like_text},
};
use portus_protocol::{
    EventObjectKind, HealthComponentType, HealthObservation, HealthReasonCode, HealthState,
    Principal, RecoveryAttempt, RecoveryDisposition,
};
use rusqlite::{OptionalExtension, params};
use std::{collections::BTreeMap, str::FromStr};

pub const MAX_HEALTH_COMPONENT_REF_BYTES: usize = 192;
pub const MAX_HEALTH_SUMMARY_BYTES: usize = 512;
pub const MAX_HEALTH_SOURCE_BYTES: usize = 128;
pub const MAX_HEALTH_GENERATION_BYTES: usize = 128;
pub const MAX_HEALTH_DETAILS: usize = 16;
pub const MAX_HEALTH_DETAIL_KEY_BYTES: usize = 64;
pub const MAX_HEALTH_DETAIL_VALUE_BYTES: usize = 256;
pub const MAX_HEALTH_DETAILS_JSON_BYTES: usize = 2048;
pub const MAX_HEALTH_OBSERVATIONS: usize = 128;
pub const MAX_RECOVERY_ATTEMPTS_PER_COMPONENT: usize = 32;
pub const RECOVERY_HISTORY_MAX_AGE_MS: i64 = 7 * 24 * 60 * 60 * 1000;
pub const HEALTH_EVENT_MAX_AGE_MS: i64 = 30 * 24 * 60 * 60 * 1000;

impl PortusState {
    pub fn record_health_observation(
        &mut self,
        observation: &HealthObservation,
    ) -> StateResult<()> {
        validate_observation(observation)?;
        let previous = self.health_observation_unfiltered(&observation.component_ref)?;
        let changed = previous.as_ref().is_none_or(|previous| {
            previous.health_state != observation.health_state
                || previous.reason_code != observation.reason_code
                || previous.recovery_disposition != observation.recovery_disposition
        });
        let details = serde_json::to_string(&observation.safe_details).map_err(|_| {
            StateError::InvalidHealthState("health safe details are not serializable".into())
        })?;
        let (uid, gid) = observation
            .owner
            .map_or((None, None), |owner| (Some(owner.uid()), Some(owner.gid())));
        self.connection.execute(
            "INSERT INTO health_observations(component_ref, owner_uid, owner_gid, component_type, health_state, reason_code, safe_summary, source, observed_at_ms, source_generation, last_healthy_at_ms, recovery_disposition, recovery_attempt_count, safe_details_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14) ON CONFLICT(component_ref) DO UPDATE SET owner_uid=excluded.owner_uid, owner_gid=excluded.owner_gid, component_type=excluded.component_type, health_state=excluded.health_state, reason_code=excluded.reason_code, safe_summary=excluded.safe_summary, source=excluded.source, observed_at_ms=excluded.observed_at_ms, source_generation=excluded.source_generation, last_healthy_at_ms=COALESCE(excluded.last_healthy_at_ms, health_observations.last_healthy_at_ms), recovery_disposition=excluded.recovery_disposition, recovery_attempt_count=excluded.recovery_attempt_count, safe_details_json=excluded.safe_details_json",
            params![
                observation.component_ref,
                uid,
                gid,
                observation.component_type.as_str(),
                observation.health_state.as_str(),
                observation.reason_code.as_str(),
                observation.summary,
                observation.source,
                observation.observed_at_ms,
                observation.source_generation,
                observation.last_healthy_at_ms,
                observation.recovery_disposition.as_str(),
                i64::from(observation.recovery_attempt_count),
                details,
            ],
        )?;
        if changed {
            self.append_significant_event(&NewSignificantEvent {
                object_kind: EventObjectKind::Health,
                object_ref: observation.component_ref.clone(),
                principal: observation.owner,
                event_kind: "health.changed".into(),
                reason_code: Some(observation.reason_code.as_str().into()),
                source_ref: Some(observation.source.clone()),
                safe_summary: Some(observation.summary.clone()),
                safe_data: serde_json::json!({
                    "health_state": observation.health_state,
                    "recovery_disposition": observation.recovery_disposition,
                }),
                occurred_at_ms: observation.observed_at_ms,
            })?;
            self.connection.execute(
                "DELETE FROM significant_events WHERE object_kind='health' AND occurred_at_ms < ?1",
                params![
                    observation
                        .observed_at_ms
                        .saturating_sub(HEALTH_EVENT_MAX_AGE_MS)
                ],
            )?;
        }
        Ok(())
    }

    pub fn health_observations_visible(
        &self,
        principal: Principal,
        degraded_only: bool,
    ) -> StateResult<Vec<HealthObservation>> {
        let sql = if degraded_only {
            "SELECT component_ref, owner_uid, owner_gid, component_type, health_state, reason_code, safe_summary, source, observed_at_ms, source_generation, last_healthy_at_ms, recovery_disposition, recovery_attempt_count, safe_details_json FROM health_observations WHERE (owner_uid IS NULL OR owner_uid=?1 OR ?1=0) AND health_state IN ('degraded','unavailable') ORDER BY component_ref LIMIT ?2"
        } else {
            "SELECT component_ref, owner_uid, owner_gid, component_type, health_state, reason_code, safe_summary, source, observed_at_ms, source_generation, last_healthy_at_ms, recovery_disposition, recovery_attempt_count, safe_details_json FROM health_observations WHERE (owner_uid IS NULL OR owner_uid=?1 OR ?1=0) ORDER BY component_ref LIMIT ?2"
        };
        let mut statement = self.connection.prepare(sql)?;
        statement
            .query_map(
                params![principal.uid(), MAX_HEALTH_OBSERVATIONS as i64],
                decode_observation,
            )?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StateError::from)
    }

    pub fn health_observation_visible(
        &self,
        component_ref: &str,
        principal: Principal,
    ) -> StateResult<Option<HealthObservation>> {
        validate_component_ref(component_ref)?;
        self.connection
            .query_row(
                "SELECT component_ref, owner_uid, owner_gid, component_type, health_state, reason_code, safe_summary, source, observed_at_ms, source_generation, last_healthy_at_ms, recovery_disposition, recovery_attempt_count, safe_details_json FROM health_observations WHERE component_ref=?1 AND (owner_uid IS NULL OR owner_uid=?2 OR ?2=0)",
                params![component_ref, principal.uid()],
                decode_observation,
            )
            .optional()
            .map_err(StateError::from)
    }

    pub fn record_recovery_attempt(&mut self, attempt: &RecoveryAttempt) -> StateResult<()> {
        validate_recovery_attempt(attempt)?;
        self.connection.execute(
            "INSERT INTO recovery_attempts(component_ref, action_kind, attempt_number, started_at_ms, finished_at_ms, outcome, reason_code, safe_summary) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                attempt.component_ref,
                attempt.action_kind.as_str(),
                i64::from(attempt.attempt_number),
                attempt.started_at_ms,
                attempt.finished_at_ms,
                attempt.outcome.as_str(),
                attempt.reason_code.as_str(),
                attempt.safe_summary,
            ],
        )?;
        let age_cutoff = attempt
            .started_at_ms
            .saturating_sub(RECOVERY_HISTORY_MAX_AGE_MS);
        self.connection.execute(
            "DELETE FROM recovery_attempts WHERE started_at_ms < ?1",
            params![age_cutoff],
        )?;
        self.connection.execute(
            "DELETE FROM recovery_attempts WHERE recovery_id IN (SELECT recovery_id FROM recovery_attempts WHERE component_ref=?1 ORDER BY started_at_ms DESC, recovery_id DESC LIMIT -1 OFFSET ?2)",
            params![attempt.component_ref, MAX_RECOVERY_ATTEMPTS_PER_COMPONENT as i64],
        )?;
        Ok(())
    }

    pub fn restart_attempt_times_since(
        &self,
        component_ref: &str,
        since_ms: i64,
    ) -> StateResult<Vec<i64>> {
        validate_component_ref(component_ref)?;
        let mut statement = self.connection.prepare(
            "SELECT started_at_ms FROM recovery_attempts WHERE component_ref=?1 AND action_kind='restart' AND started_at_ms>=?2 ORDER BY started_at_ms ASC",
        )?;
        statement
            .query_map(params![component_ref, since_ms], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StateError::from)
    }

    fn health_observation_unfiltered(
        &self,
        component_ref: &str,
    ) -> StateResult<Option<HealthObservation>> {
        self.connection
            .query_row(
                "SELECT component_ref, owner_uid, owner_gid, component_type, health_state, reason_code, safe_summary, source, observed_at_ms, source_generation, last_healthy_at_ms, recovery_disposition, recovery_attempt_count, safe_details_json FROM health_observations WHERE component_ref=?1",
                params![component_ref],
                decode_observation,
            )
            .optional()
            .map_err(StateError::from)
    }
}

fn decode_observation(row: &rusqlite::Row<'_>) -> rusqlite::Result<HealthObservation> {
    let uid: Option<u32> = row.get(1)?;
    let gid: Option<u32> = row.get(2)?;
    let owner = match (uid, gid) {
        (Some(uid), Some(gid)) => Some(Principal::new(uid, gid)),
        (None, None) => None,
        _ => return Err(rusqlite::Error::InvalidQuery),
    };
    let component_type = HealthComponentType::from_str(&row.get::<_, String>(3)?)
        .map_err(|error| from_text_error(3, error))?;
    let health_state =
        parse_health_state(&row.get::<_, String>(4)?).ok_or(rusqlite::Error::InvalidQuery)?;
    let reason_code = HealthReasonCode::from_str(&row.get::<_, String>(5)?)
        .map_err(|error| from_text_error(5, error))?;
    let recovery_disposition = parse_recovery_disposition(&row.get::<_, String>(11)?)
        .ok_or(rusqlite::Error::InvalidQuery)?;
    let count: i64 = row.get(12)?;
    let details_encoded: String = row.get(13)?;
    let safe_details =
        serde_json::from_str::<BTreeMap<String, String>>(&details_encoded).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                13,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
    Ok(HealthObservation {
        component_ref: row.get(0)?,
        owner,
        component_type,
        health_state,
        reason_code,
        summary: row.get(6)?,
        source: row.get(7)?,
        observed_at_ms: row.get(8)?,
        source_generation: row.get(9)?,
        last_healthy_at_ms: row.get(10)?,
        recovery_disposition,
        recovery_attempt_count: u16::try_from(count).unwrap_or(u16::MAX),
        safe_details,
    })
}

fn from_text_error(
    index: usize,
    error: impl std::error::Error + Send + Sync + 'static,
) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(index, rusqlite::types::Type::Text, Box::new(error))
}

fn parse_health_state(value: &str) -> Option<HealthState> {
    match value {
        "healthy" => Some(HealthState::Healthy),
        "degraded" => Some(HealthState::Degraded),
        "unavailable" => Some(HealthState::Unavailable),
        "unknown" => Some(HealthState::Unknown),
        _ => None,
    }
}

fn parse_recovery_disposition(value: &str) -> Option<RecoveryDisposition> {
    match value {
        "observe" => Some(RecoveryDisposition::Observe),
        "reconcile" => Some(RecoveryDisposition::Reconcile),
        "restart" => Some(RecoveryDisposition::Restart),
        "repair" => Some(RecoveryDisposition::Repair),
        "administrator_required" => Some(RecoveryDisposition::AdministratorRequired),
        "terminal" => Some(RecoveryDisposition::Terminal),
        _ => None,
    }
}

fn validate_observation(observation: &HealthObservation) -> StateResult<()> {
    validate_component_ref(&observation.component_ref)?;
    validate_text(&observation.summary, MAX_HEALTH_SUMMARY_BYTES, "summary")?;
    validate_text(&observation.source, MAX_HEALTH_SOURCE_BYTES, "source")?;
    if secret_like_text(&observation.summary) || secret_like_text(&observation.source) {
        return Err(StateError::InvalidHealthState(
            "health observation contains secret-like text".into(),
        ));
    }
    if let Some(generation) = observation.source_generation.as_deref() {
        validate_text(generation, MAX_HEALTH_GENERATION_BYTES, "source generation")?;
        if secret_like_text(generation) {
            return Err(StateError::InvalidHealthState(
                "health source generation contains secret-like text".into(),
            ));
        }
    }
    if observation.safe_details.len() > MAX_HEALTH_DETAILS {
        return Err(StateError::InvalidHealthState(
            "too many health safe-detail fields".into(),
        ));
    }
    for (key, value) in &observation.safe_details {
        validate_text(key, MAX_HEALTH_DETAIL_KEY_BYTES, "safe-detail key")?;
        validate_text(value, MAX_HEALTH_DETAIL_VALUE_BYTES, "safe-detail value")?;
        if secret_like_key(key) || secret_like_text(value) {
            return Err(StateError::InvalidHealthState(
                "secret-like health safe-detail material is forbidden".into(),
            ));
        }
    }
    let encoded = serde_json::to_string(&observation.safe_details).map_err(|_| {
        StateError::InvalidHealthState("health safe details are not serializable".into())
    })?;
    if encoded.len() > MAX_HEALTH_DETAILS_JSON_BYTES {
        return Err(StateError::InvalidHealthState(
            "health safe details exceed bounded size".into(),
        ));
    }
    Ok(())
}

fn validate_recovery_attempt(attempt: &RecoveryAttempt) -> StateResult<()> {
    validate_component_ref(&attempt.component_ref)?;
    if attempt.attempt_number == 0 {
        return Err(StateError::InvalidHealthState(
            "recovery attempt number must be positive".into(),
        ));
    }
    if let Some(summary) = attempt.safe_summary.as_deref() {
        validate_text(summary, MAX_HEALTH_SUMMARY_BYTES, "recovery summary")?;
        if secret_like_text(summary) {
            return Err(StateError::InvalidHealthState(
                "recovery summary contains secret-like text".into(),
            ));
        }
    }
    Ok(())
}

fn validate_component_ref(value: &str) -> StateResult<()> {
    validate_text(value, MAX_HEALTH_COMPONENT_REF_BYTES, "component reference")
}

fn validate_text(value: &str, max: usize, field: &str) -> StateResult<()> {
    if value.trim().is_empty() || value.len() > max || value.contains(['\0', '\n', '\r']) {
        Err(StateError::InvalidHealthState(format!(
            "{field} is invalid"
        )))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use portus_protocol::{
        HealthComponentType, HealthReasonCode, RecoveryActionKind, RecoveryAttemptOutcome,
    };
    use std::fs;

    struct TestDb {
        path: std::path::PathBuf,
        dir: std::path::PathBuf,
    }

    impl TestDb {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "portus-health-state-{name}-{}",
                portus_protocol::TaskId::new()
            ));
            fs::create_dir_all(&dir).unwrap();
            Self {
                path: dir.join("portus.db"),
                dir,
            }
        }
    }
    impl Drop for TestDb {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    fn observation(
        component_ref: &str,
        state: HealthState,
        reason: HealthReasonCode,
        at: i64,
    ) -> HealthObservation {
        HealthObservation {
            component_ref: component_ref.into(),
            owner: None,
            component_type: HealthComponentType::Runtime,
            health_state: state,
            reason_code: reason,
            summary: "bounded health summary".into(),
            source: "fixture".into(),
            observed_at_ms: at,
            source_generation: None,
            last_healthy_at_ms: (state == HealthState::Healthy).then_some(at),
            recovery_disposition: RecoveryDisposition::Observe,
            recovery_attempt_count: 0,
            safe_details: BTreeMap::new(),
        }
    }

    #[test]
    fn current_health_upserts_and_change_history_is_significant_only() {
        let test = TestDb::new("upsert");
        let mut state = PortusState::open(&test.path).unwrap();
        state
            .record_health_observation(&observation(
                "runtime:portusd",
                HealthState::Healthy,
                HealthReasonCode::Ready,
                10,
            ))
            .unwrap();
        state
            .record_health_observation(&observation(
                "runtime:portusd",
                HealthState::Healthy,
                HealthReasonCode::Ready,
                11,
            ))
            .unwrap();
        state
            .record_health_observation(&observation(
                "runtime:portusd",
                HealthState::Degraded,
                HealthReasonCode::IpcFailed,
                12,
            ))
            .unwrap();
        let current = state
            .health_observation_visible("runtime:portusd", Principal::new(1000, 1000))
            .unwrap()
            .unwrap();
        assert_eq!(current.health_state, HealthState::Degraded);
        let events = state
            .significant_events_for_object(EventObjectKind::Health, "runtime:portusd", None, 10)
            .unwrap();
        assert_eq!(events.events.len(), 2);
    }

    #[test]
    fn principal_filtering_and_secret_like_details_fail_closed() {
        let test = TestDb::new("principal");
        let mut state = PortusState::open(&test.path).unwrap();
        let mut item = observation(
            "index-source:x11:uid1000",
            HealthState::Healthy,
            HealthReasonCode::Ready,
            10,
        );
        item.owner = Some(Principal::new(1000, 1000));
        state.record_health_observation(&item).unwrap();
        assert!(
            state
                .health_observation_visible(&item.component_ref, Principal::new(1001, 1001))
                .unwrap()
                .is_none()
        );
        assert!(
            state
                .health_observation_visible(&item.component_ref, Principal::new(0, 0))
                .unwrap()
                .is_some()
        );
        item.safe_details
            .insert("api_key".into(), "forbidden".into());
        assert!(state.record_health_observation(&item).is_err());
        item.safe_details.clear();
        item.safe_details
            .insert("note".into(), "Bearer do-not-store".into());
        assert!(state.record_health_observation(&item).is_err());
    }

    #[test]
    fn recovery_history_is_bounded_by_count_and_age() {
        let test = TestDb::new("recovery");
        let mut state = PortusState::open(&test.path).unwrap();
        for number in 1..=40_u16 {
            let at = i64::from(number) * 1_000;
            state
                .record_recovery_attempt(&RecoveryAttempt {
                    component_ref: "service:portusd".into(),
                    action_kind: RecoveryActionKind::Restart,
                    attempt_number: number,
                    started_at_ms: at,
                    finished_at_ms: Some(at + 1),
                    outcome: RecoveryAttemptOutcome::Failed,
                    reason_code: HealthReasonCode::ServiceNotRunning,
                    safe_summary: Some("fixture restart".into()),
                })
                .unwrap();
        }
        let attempts = state
            .restart_attempt_times_since("service:portusd", 0)
            .unwrap();
        assert_eq!(attempts.len(), MAX_RECOVERY_ATTEMPTS_PER_COMPONENT);
    }
}
