use portus_protocol::{
    HealthState, IndexObservationInput, IndexRelationInput, IndexSourceKind, IndexSourceStatus,
    Principal,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IndexRescanDomain {
    Applications,
    Runtime,
    Providers,
    Services,
    All,
}

impl IndexRescanDomain {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Applications => "applications",
            Self::Runtime => "runtime",
            Self::Providers => "providers",
            Self::Services => "services",
            Self::All => "all",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SourceBatch {
    pub status: IndexSourceStatus,
    pub observations: Vec<IndexObservationInput>,
    pub relations: Vec<IndexRelationInput>,
}

impl SourceBatch {
    #[must_use]
    pub fn unavailable(
        source_id: impl Into<String>,
        source_kind: IndexSourceKind,
        owner: Option<Principal>,
        generation: impl Into<String>,
        reason_code: impl Into<String>,
        observed_at_ms: i64,
    ) -> Self {
        Self {
            status: IndexSourceStatus {
                source_id: source_id.into(),
                source_kind,
                owner,
                source_generation: generation.into(),
                health: HealthState::Unavailable,
                reason_code: reason_code.into(),
                last_attempt_at_ms: observed_at_ms,
                last_success_at_ms: None,
            },
            observations: Vec::new(),
            relations: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SourceCollection {
    pub batches: Vec<SourceBatch>,
}

pub trait IndexSourceSet: Send + Sync {
    fn collect(
        &self,
        domain: IndexRescanDomain,
        principal: Principal,
        observed_at_ms: i64,
    ) -> SourceCollection;
}

#[derive(Default)]
pub struct DisabledIndexSources;

impl IndexSourceSet for DisabledIndexSources {
    fn collect(
        &self,
        domain: IndexRescanDomain,
        principal: Principal,
        observed_at_ms: i64,
    ) -> SourceCollection {
        let mut batches = Vec::new();
        if matches!(
            domain,
            IndexRescanDomain::Applications | IndexRescanDomain::All
        ) {
            batches.push(SourceBatch::unavailable(
                "applications",
                IndexSourceKind::Applications,
                None,
                "disabled",
                "source_disabled",
                observed_at_ms,
            ));
        }
        if matches!(domain, IndexRescanDomain::Runtime | IndexRescanDomain::All) {
            batches.push(SourceBatch::unavailable(
                "proc",
                IndexSourceKind::Proc,
                None,
                "disabled",
                "source_disabled",
                observed_at_ms,
            ));
            batches.push(SourceBatch::unavailable(
                format!("i3:{}", principal.uid()),
                IndexSourceKind::I3,
                Some(principal),
                "disabled",
                "source_disabled",
                observed_at_ms,
            ));
            batches.push(SourceBatch::unavailable(
                format!("x11:{}", principal.uid()),
                IndexSourceKind::X11,
                Some(principal),
                "disabled",
                "source_disabled",
                observed_at_ms,
            ));
        }
        if matches!(domain, IndexRescanDomain::Services | IndexRescanDomain::All) {
            batches.push(SourceBatch::unavailable(
                "openrc",
                IndexSourceKind::OpenRc,
                None,
                "disabled",
                "source_disabled",
                observed_at_ms,
            ));
        }
        SourceCollection { batches }
    }
}
