//! PortusOS durable and derived state layer.
//!
//! This crate owns SQLite opening policy, schema migration, integrity/readiness
//! checks, principal-scoped access helpers, and bounded cleanup primitives. It
//! does not own higher-level task/provider/index business logic.

mod artifact;
mod error;
mod event;
mod health;
mod index;
mod migration;
mod provider;
mod safety;
mod schema;
mod store;
mod task;

pub use artifact::{
    ArtifactCleanupEligibility, MAX_ARTIFACT_LIST_PAGE, MAX_ARTIFACT_METADATA_FIELDS,
    MAX_ARTIFACT_METADATA_JSON_BYTES, MAX_ARTIFACT_SHARED_PRINCIPALS,
};
pub use error::{StateError, StateResult};
pub use event::{
    MAX_SIGNIFICANT_EVENT_DATA_BYTES, MAX_SIGNIFICANT_EVENT_KIND_BYTES, MAX_SIGNIFICANT_EVENT_PAGE,
    MAX_SIGNIFICANT_EVENT_REASON_BYTES, MAX_SIGNIFICANT_EVENT_REF_BYTES,
    MAX_SIGNIFICANT_EVENT_SUMMARY_BYTES, MAX_SIGNIFICANT_EVENTS_PER_OBJECT, NewSignificantEvent,
};
pub use health::{
    HEALTH_EVENT_MAX_AGE_MS, MAX_HEALTH_COMPONENT_REF_BYTES, MAX_HEALTH_DETAILS,
    MAX_HEALTH_DETAILS_JSON_BYTES, MAX_HEALTH_OBSERVATIONS, MAX_HEALTH_SOURCE_BYTES,
    MAX_HEALTH_SUMMARY_BYTES, MAX_RECOVERY_ATTEMPTS_PER_COMPONENT, RECOVERY_HISTORY_MAX_AGE_MS,
};
pub use index::{
    IndexQueryFilter, IndexRuntimeStatus, IndexTopologyView, IndexView, MAX_INDEX_QUERY_SCAN,
};
pub use migration::{LATEST_SCHEMA_VERSION, MigrationInfo, migration_plan};
pub use provider::{
    CapabilityProviderView, CapabilityView, MAX_PROVIDER_RESOURCE_VIEWS,
    MAX_PROVIDER_RUNTIME_REASON_BYTES, ProviderCapabilityRuntimeSpec, ProviderCapabilitySpec,
    ProviderInterfaceSpec, ProviderPage, ProviderReconcileResult, ProviderRegistrationRecord,
    ProviderRegistrationSpec, ProviderResourceRuntimeSpec, ProviderResourceTypeSpec,
    ProviderResourceView, ProviderRuntimeStatusSpec, ProviderTombstoneView, ProviderView,
};
pub use store::{
    DEFAULT_BUSY_TIMEOUT, DatabaseReadiness, PortusState, PrincipalTaskRecord, StateOpenOptions,
};
pub use task::{
    MAX_TASK_EVENT_DATA_BYTES, MAX_TASK_EVENT_SUMMARY_BYTES, MAX_TASK_OBJECTIVE_BYTES,
    MAX_TASK_REASON_BYTES, MAX_TASK_REF_BYTES, MAX_TASK_RESULT_BYTES, MAX_TASK_TITLE_BYTES,
    NewExecutionRelationship, NewTaskRecord, TaskListFilter, TaskTransition,
};

/// Canonical installed-system path. Tests and development-host integration must
/// use explicit isolated paths instead of touching this location.
pub const CANONICAL_DATABASE_PATH: &str = "/var/lib/portus/state/portus.db";
