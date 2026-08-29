use crate::{
    PortusState, StateError, StateResult,
    safety::{json_contains_secret_like, secret_like_text},
};
use portus_protocol::{EventObjectKind, Principal, SignificantEvent, SignificantEventPage};
use rusqlite::{Transaction, params};
use serde_json::Value;

pub const MAX_SIGNIFICANT_EVENTS_PER_OBJECT: u64 = 512;
pub const MAX_SIGNIFICANT_EVENT_KIND_BYTES: usize = 128;
pub const MAX_SIGNIFICANT_EVENT_REASON_BYTES: usize = 256;
pub const MAX_SIGNIFICANT_EVENT_REF_BYTES: usize = 512;
pub const MAX_SIGNIFICANT_EVENT_SUMMARY_BYTES: usize = 1024;
pub const MAX_SIGNIFICANT_EVENT_DATA_BYTES: usize = 4096;
pub const MAX_SIGNIFICANT_EVENT_PAGE: u16 = 200;

#[derive(Clone, Debug)]
pub struct NewSignificantEvent {
    pub object_kind: EventObjectKind,
    pub object_ref: String,
    pub principal: Option<Principal>,
    pub event_kind: String,
    pub reason_code: Option<String>,
    pub source_ref: Option<String>,
    pub safe_summary: Option<String>,
    pub safe_data: Value,
    pub occurred_at_ms: i64,
}

impl PortusState {
    pub fn append_significant_event(
        &mut self,
        event: &NewSignificantEvent,
    ) -> StateResult<SignificantEvent> {
        validate_event(event)?;
        let tx = self.connection.transaction()?;
        let next: i64 = tx.query_row(
            "SELECT COALESCE(MAX(object_sequence), 0) + 1 FROM significant_events WHERE object_kind=?1 AND object_ref=?2",
            params![event.object_kind.as_str(), event.object_ref],
            |row| row.get(0),
        )?;
        let sequence = u64::try_from(next).map_err(|_| {
            StateError::InvalidEventState("significant event sequence overflow".into())
        })?;
        insert_significant_event_tx(&tx, event, sequence)?;
        prune_object_tx(&tx, event.object_kind, &event.object_ref, sequence)?;
        tx.commit()?;
        Ok(SignificantEvent {
            object_kind: event.object_kind,
            object_ref: event.object_ref.clone(),
            sequence,
            principal: event.principal,
            event_kind: event.event_kind.clone(),
            reason_code: event.reason_code.clone(),
            source_ref: event.source_ref.clone(),
            safe_summary: event.safe_summary.clone(),
            safe_data: event.safe_data.clone(),
            occurred_at_ms: event.occurred_at_ms,
        })
    }

    pub fn significant_events_for_object(
        &self,
        object_kind: EventObjectKind,
        object_ref: &str,
        after_sequence: Option<u64>,
        limit: u16,
    ) -> StateResult<SignificantEventPage> {
        if object_ref.trim().is_empty() || object_ref.len() > MAX_SIGNIFICANT_EVENT_REF_BYTES {
            return Err(StateError::InvalidEventState(
                "event object reference is invalid".into(),
            ));
        }
        if limit == 0 || limit > MAX_SIGNIFICANT_EVENT_PAGE {
            return Err(StateError::InvalidEventState(
                "significant event limit must be between 1 and 200".into(),
            ));
        }
        let retained_from: Option<i64> = self.connection.query_row(
            "SELECT MIN(object_sequence) FROM significant_events WHERE object_kind=?1 AND object_ref=?2",
            params![object_kind.as_str(), object_ref],
            |row| row.get(0),
        )?;
        let latest: Option<i64> = self.connection.query_row(
            "SELECT MAX(object_sequence) FROM significant_events WHERE object_kind=?1 AND object_ref=?2",
            params![object_kind.as_str(), object_ref],
            |row| row.get(0),
        )?;
        let after = after_sequence.unwrap_or(0).min(i64::MAX as u64) as i64;
        let mut statement = self.connection.prepare(
            "SELECT object_sequence, principal_uid, principal_gid, event_kind, reason_code, source_ref, safe_summary, safe_data_json, occurred_at_ms FROM significant_events WHERE object_kind=?1 AND object_ref=?2 AND object_sequence>?3 ORDER BY object_sequence ASC LIMIT ?4",
        )?;
        let rows = statement
            .query_map(
                params![
                    object_kind.as_str(),
                    object_ref,
                    after,
                    i64::from(limit) + 1
                ],
                |row| decode_event_row(row, object_kind, object_ref),
            )?
            .collect::<Result<Vec<_>, _>>()?;
        let has_more = rows.len() > usize::from(limit);
        let events = rows
            .into_iter()
            .take(usize::from(limit))
            .collect::<Vec<_>>();
        let retained_from_sequence = retained_from.and_then(|value| u64::try_from(value).ok());
        let latest_sequence = latest
            .and_then(|value| u64::try_from(value).ok())
            .unwrap_or(0);
        let gap_before_page = match (after_sequence, retained_from_sequence) {
            (Some(after), Some(first)) => after.saturating_add(1) < first,
            _ => false,
        };
        let next_sequence = has_more
            .then(|| events.last().map(|event| event.sequence))
            .flatten();
        Ok(SignificantEventPage {
            events,
            retained_from_sequence,
            latest_sequence,
            gap_before_page,
            next_sequence,
        })
    }
}

pub(crate) fn insert_significant_event_tx(
    tx: &Transaction<'_>,
    event: &NewSignificantEvent,
    sequence: u64,
) -> StateResult<()> {
    validate_event(event)?;
    let encoded = serde_json::to_string(&event.safe_data)
        .map_err(|_| StateError::InvalidEventState("event data is not serializable".into()))?;
    if encoded.len() > MAX_SIGNIFICANT_EVENT_DATA_BYTES {
        return Err(StateError::InvalidEventState(
            "significant event data exceeds bounded size".into(),
        ));
    }
    let (uid, gid) = event.principal.map_or((None, None), |principal| {
        (Some(principal.uid()), Some(principal.gid()))
    });
    tx.execute(
        "INSERT INTO significant_events(object_kind, object_ref, object_sequence, principal_uid, principal_gid, event_kind, reason_code, source_ref, safe_summary, safe_data_json, occurred_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            event.object_kind.as_str(),
            event.object_ref,
            i64::try_from(sequence).unwrap_or(i64::MAX),
            uid,
            gid,
            event.event_kind,
            event.reason_code,
            event.source_ref,
            event.safe_summary,
            encoded,
            event.occurred_at_ms,
        ],
    )?;
    Ok(())
}

pub(crate) fn prune_object_tx(
    tx: &Transaction<'_>,
    object_kind: EventObjectKind,
    object_ref: &str,
    latest_sequence: u64,
) -> StateResult<()> {
    let cutoff = latest_sequence.saturating_sub(MAX_SIGNIFICANT_EVENTS_PER_OBJECT);
    if cutoff == 0 {
        return Ok(());
    }
    tx.execute(
        "DELETE FROM significant_events WHERE object_kind=?1 AND object_ref=?2 AND object_sequence<=?3",
        params![object_kind.as_str(), object_ref, i64::try_from(cutoff).unwrap_or(i64::MAX)],
    )?;
    Ok(())
}

fn validate_event(event: &NewSignificantEvent) -> StateResult<()> {
    validate_nonempty(
        &event.object_ref,
        MAX_SIGNIFICANT_EVENT_REF_BYTES,
        "object reference",
    )?;
    validate_nonempty(
        &event.event_kind,
        MAX_SIGNIFICANT_EVENT_KIND_BYTES,
        "event kind",
    )?;
    validate_optional(
        event.reason_code.as_deref(),
        MAX_SIGNIFICANT_EVENT_REASON_BYTES,
        "reason code",
    )?;
    validate_optional(
        event.source_ref.as_deref(),
        MAX_SIGNIFICANT_EVENT_REF_BYTES,
        "source reference",
    )?;
    validate_optional(
        event.safe_summary.as_deref(),
        MAX_SIGNIFICANT_EVENT_SUMMARY_BYTES,
        "safe summary",
    )?;
    for value in [
        Some(event.object_ref.as_str()),
        Some(event.event_kind.as_str()),
        event.reason_code.as_deref(),
        event.source_ref.as_deref(),
        event.safe_summary.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if secret_like_text(value) {
            return Err(StateError::InvalidEventState(
                "significant event contains secret-like text".into(),
            ));
        }
    }
    if json_contains_secret_like(&event.safe_data) {
        return Err(StateError::InvalidEventState(
            "significant event safe data contains secret-like material".into(),
        ));
    }
    let encoded = serde_json::to_string(&event.safe_data)
        .map_err(|_| StateError::InvalidEventState("event data is not serializable".into()))?;
    if encoded.len() > MAX_SIGNIFICANT_EVENT_DATA_BYTES {
        return Err(StateError::InvalidEventState(
            "event data exceeds bounded size".into(),
        ));
    }
    Ok(())
}

fn validate_nonempty(value: &str, max: usize, field: &str) -> StateResult<()> {
    if value.trim().is_empty() || value.len() > max || value.contains(['\0', '\n', '\r']) {
        Err(StateError::InvalidEventState(format!("{field} is invalid")))
    } else {
        Ok(())
    }
}

fn validate_optional(value: Option<&str>, max: usize, field: &str) -> StateResult<()> {
    match value {
        Some(value) => validate_nonempty(value, max, field),
        None => Ok(()),
    }
}

fn decode_event_row(
    row: &rusqlite::Row<'_>,
    object_kind: EventObjectKind,
    object_ref: &str,
) -> rusqlite::Result<SignificantEvent> {
    let sequence: i64 = row.get(0)?;
    let uid: Option<u32> = row.get(1)?;
    let gid: Option<u32> = row.get(2)?;
    let principal = match (uid, gid) {
        (Some(uid), Some(gid)) => Some(Principal::new(uid, gid)),
        (None, None) => None,
        _ => {
            return Err(rusqlite::Error::InvalidColumnType(
                1,
                "principal".into(),
                rusqlite::types::Type::Null,
            ));
        }
    };
    let encoded: String = row.get(7)?;
    let safe_data = serde_json::from_str::<Value>(&encoded).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(7, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(SignificantEvent {
        object_kind,
        object_ref: object_ref.to_string(),
        sequence: u64::try_from(sequence).unwrap_or(0),
        principal,
        event_kind: row.get(3)?,
        reason_code: row.get(4)?,
        source_ref: row.get(5)?,
        safe_summary: row.get(6)?,
        safe_data,
        occurred_at_ms: row.get(8)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PortusState;
    use serde_json::json;
    use std::{fs, path::PathBuf};

    struct TestState {
        dir: PathBuf,
        state: PortusState,
    }

    impl TestState {
        fn new() -> Self {
            let dir = std::env::temp_dir()
                .join(format!("portus-events-{}", portus_protocol::TaskId::new()));
            fs::create_dir_all(&dir).unwrap();
            let state = PortusState::open(dir.join("portus.db")).unwrap();
            Self { dir, state }
        }
    }

    impl Drop for TestState {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    fn event(index: u64) -> NewSignificantEvent {
        NewSignificantEvent {
            object_kind: EventObjectKind::Policy,
            object_ref: "policy:uid:1000".into(),
            principal: Some(Principal::new(1000, 1000)),
            event_kind: "policy.fixture".into(),
            reason_code: Some("test".into()),
            source_ref: Some("test".into()),
            safe_summary: Some(format!("event {index}")),
            safe_data: json!({"index":index}),
            occurred_at_ms: index as i64,
        }
    }

    #[test]
    fn common_events_are_ordered_and_attributable() {
        let mut test = TestState::new();
        let first = test.state.append_significant_event(&event(1)).unwrap();
        let second = test.state.append_significant_event(&event(2)).unwrap();
        assert_eq!(first.sequence, 1);
        assert_eq!(second.sequence, 2);
        let page = test
            .state
            .significant_events_for_object(EventObjectKind::Policy, "policy:uid:1000", None, 10)
            .unwrap();
        assert_eq!(page.events.len(), 2);
        assert_eq!(page.events[0].principal, Some(Principal::new(1000, 1000)));
        assert_eq!(page.latest_sequence, 2);
        assert!(!page.gap_before_page);
    }

    #[test]
    fn retention_is_bounded_and_gap_is_explicit() {
        let mut test = TestState::new();
        for index in 1..=(MAX_SIGNIFICANT_EVENTS_PER_OBJECT + 20) {
            test.state.append_significant_event(&event(index)).unwrap();
        }
        let page = test
            .state
            .significant_events_for_object(EventObjectKind::Policy, "policy:uid:1000", Some(1), 200)
            .unwrap();
        assert_eq!(page.retained_from_sequence, Some(21));
        assert_eq!(page.latest_sequence, MAX_SIGNIFICANT_EVENTS_PER_OBJECT + 20);
        assert!(page.gap_before_page);
        let count: i64 = test.state.connection.query_row(
            "SELECT COUNT(*) FROM significant_events WHERE object_kind='policy' AND object_ref='policy:uid:1000'",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(count as u64, MAX_SIGNIFICANT_EVENTS_PER_OBJECT);
    }

    #[test]
    fn secret_like_event_fields_and_nested_safe_data_are_rejected() {
        let mut test = TestState::new();
        let mut unsafe_summary = event(1);
        unsafe_summary.safe_summary = Some("Authorization: Bearer do-not-store".into());
        assert!(matches!(
            test.state.append_significant_event(&unsafe_summary),
            Err(StateError::InvalidEventState(_))
        ));

        let mut unsafe_data = event(2);
        unsafe_data.safe_data = json!({"nested":{"access_token":"do-not-store"}});
        assert!(matches!(
            test.state.append_significant_event(&unsafe_data),
            Err(StateError::InvalidEventState(_))
        ));

        let mut unsafe_value = event(3);
        unsafe_value.safe_data = json!({"note":"token=do-not-store"});
        assert!(matches!(
            test.state.append_significant_event(&unsafe_value),
            Err(StateError::InvalidEventState(_))
        ));
    }

    #[test]
    fn oversized_or_multiline_event_metadata_is_rejected() {
        let mut test = TestState::new();
        let mut invalid = event(1);
        invalid.event_kind = "bad\nkind".into();
        assert!(matches!(
            test.state.append_significant_event(&invalid),
            Err(StateError::InvalidEventState(_))
        ));
        let mut huge = event(2);
        huge.safe_data = json!({"trace":"x".repeat(MAX_SIGNIFICANT_EVENT_DATA_BYTES + 1)});
        assert!(matches!(
            test.state.append_significant_event(&huge),
            Err(StateError::InvalidEventState(_))
        ));
    }
}
