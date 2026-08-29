//! Shared PortusOS wire, identifier, principal, and semantic types.
//!
//! This crate owns the common language-level contract used across first-party
//! PortusOS components. It intentionally contains no socket transport, state
//! database, policy engine, provider-domain behavior, or CLI rendering logic.

mod artifact;
mod event;
mod health;
mod ids;
mod index;
mod pagination;
mod policy;
mod principal;
mod provider;
mod redaction;
mod semantics;
mod task;
mod wire;

pub use artifact::{
    ArtifactAvailabilityState, ArtifactCleanupAuthority, ArtifactConfidentiality, ArtifactHold,
    ArtifactHoldKind, ArtifactIntegrityKind, ArtifactLocator, ArtifactPage, ArtifactRecord,
    ArtifactRegistrationSpec, ArtifactRetentionKind, ArtifactSummary, ArtifactTaskRelationship,
    ArtifactTaskRelationshipKind, ArtifactType, ArtifactView,
};
pub use event::{
    AuditActor, AuditActorKind, AuditDomain, AuditRecord, AuditResult, EventObjectKind,
    SignificantEvent, SignificantEventPage, TaskEventStreamFrame, TaskEventStreamFrameKind,
};
pub use health::{
    HealthComponentType, HealthEnumParseError, HealthObservation, HealthReasonCode,
    RecoveryActionKind, RecoveryAttempt, RecoveryAttemptOutcome,
};
pub use ids::{ArtifactId, IdParseError, IndexHandle, ProviderRegistrationId, RequestId, TaskId};
pub use index::{
    ControlPathKind, IndexHealthState, IndexObservation, IndexObservationInput, IndexPage,
    IndexRelation, IndexRelationInput, IndexResourceType, IndexSourceKind, IndexSourceStatus,
};
pub use pagination::{OpaqueCursor, PageLimit, PageRequest, PaginationError};
pub use policy::{
    EffectiveBundleView, EffectiveGrantView, EffectivePolicyView, PolicyActionContext,
    PolicyDecision, PolicyEffect, PolicyEnforcementClass, PrivilegedOperationRequest,
    PrivilegedOperationResult, SubjectPolicyView,
};
pub use principal::Principal;
pub use provider::{ProviderResourceId, ProviderResourceRef, ResourceType, ResourceValueError};
pub use redaction::Redacted;
pub use semantics::{
    EvidenceStrength, Freshness, HealthState, RecoveryDisposition, SemanticErrorCode,
};
pub use task::{
    ExecutionRelationshipMode, ExecutionRelationshipStatus, ProjectRecord, RetrySafety,
    SessionReference, TaskBackendKind, TaskEvent, TaskEventPage, TaskExecutionRelationship,
    TaskPage, TaskResultKind, TaskState, TaskSummary, TaskView, WaitingReason,
};
pub use wire::{
    CURRENT_PROTOCOL_VERSION, ProtocolError, ProtocolVersion, RequestEnvelope, ResponseEnvelope,
    SemanticError,
};
