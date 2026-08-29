//! Strict first-ISO administrator-policy parser and resolver.
//!
//! Policy is declarative and typed. This crate never executes privileged work.

mod model;
mod mutation;
mod snapshot;

pub use model::{
    ActionDefinition, ActionRegistry, BundleDefinition, BundleSelection, GlobalPolicy,
    GrantDefinition, POLICY_VERSION, PolicyError, PolicyPaths, PolicyResult, PolicyTrust,
    SubjectPolicy,
};
pub use mutation::{AdminMutation, apply_admin_mutation, serialize_subject};
pub use snapshot::PolicySnapshot;

pub const CANONICAL_POLICY_PATH: &str = "/etc/portus/policy/policy.toml";
pub const CANONICAL_SUBJECTS_DIR: &str = "/etc/portus/policy/subjects.d";
pub const CANONICAL_ACTIONS_PATH: &str = "/usr/share/portus/policy/actions.toml";
pub const CANONICAL_BUNDLES_DIR: &str = "/usr/share/portus/policy/bundles";
