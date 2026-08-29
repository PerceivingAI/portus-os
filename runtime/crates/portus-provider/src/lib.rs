//! Trusted first-class provider manifest parsing and registration reconciliation.
//!
//! This crate intentionally does not invoke provider operations. It turns
//! administrator-controlled declarative manifests into bounded Portus registry
//! state while preserving provider-owned interfaces and resources.

mod error;
mod manifest;
mod reconcile;

pub use error::{ProviderError, ProviderResult};
pub use manifest::{
    CapabilityManifest, HealthIntegrationKind, HealthManifest, InterfaceManifest, InterfaceType,
    LifecycleManifest, LifecycleOwner, MANIFEST_SCHEMA_VERSION, PolicyManifest, ProviderManifest,
    ProviderScope, ProviderSection, ResourceAuthority, ResourceLifetime, ResourceManifest,
};
pub use reconcile::{
    CANONICAL_MANIFEST_DIR, MAX_MANIFEST_BYTES, MAX_MANIFESTS, ManifestTrust, ReconcileReport,
    fixture_manifest_path, reconcile_directory, reconcile_directory_at,
};
