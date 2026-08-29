//! Protected reusable API credential provider service.
//!
//! `portus-apid` owns the reusable credential store and authenticated upstream
//! request construction. It deliberately exposes no raw-secret retrieval API.

mod authorizer;
mod core;
mod store;
mod upstream;

#[cfg(target_os = "linux")]
mod unix;

pub use authorizer::{FilesystemPolicyAuthorizer, ProtectedApiAuthorizer};
pub use core::ProtectedApiCore;
pub use store::{CredentialStore, STORE_SCHEMA_VERSION, StoreError, StoreResult};
pub use upstream::{HttpsUpstream, UpstreamRequest, UpstreamResponse, UpstreamTransport};

#[cfg(target_os = "linux")]
pub use unix::{ProtectedApiServer, ProtectedApiServerConfig, ServiceIdentityTrust};
