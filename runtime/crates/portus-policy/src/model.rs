use portus_protocol::{PolicyEffect, PolicyEnforcementClass};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt, io, path::PathBuf};

pub const POLICY_VERSION: u32 = 1;
pub const MAX_ACTIONS: usize = 128;
pub const MAX_BUNDLES: usize = 32;
pub const MAX_SUBJECTS: usize = 1024;
pub const MAX_GRANTS: usize = 256;
pub const MAX_RESOURCES_PER_GRANT: usize = 128;
pub const MAX_ID_BYTES: usize = 96;
pub const MAX_LABEL_BYTES: usize = 128;
pub const MAX_RESOURCE_BYTES: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolicyTrust {
    RootOwnedSystem,
    PretrustedFixture,
}

#[derive(Clone, Debug)]
pub struct PolicyPaths {
    pub policy_path: PathBuf,
    pub subjects_dir: PathBuf,
    pub actions_path: PathBuf,
    pub bundles_dir: PathBuf,
}

impl PolicyPaths {
    #[must_use]
    pub fn canonical() -> Self {
        Self {
            policy_path: crate::CANONICAL_POLICY_PATH.into(),
            subjects_dir: crate::CANONICAL_SUBJECTS_DIR.into(),
            actions_path: crate::CANONICAL_ACTIONS_PATH.into(),
            bundles_dir: crate::CANONICAL_BUNDLES_DIR.into(),
        }
    }

    #[must_use]
    pub fn subject_path(&self, uid: u32) -> PathBuf {
        self.subjects_dir.join(format!("{uid}.toml"))
    }
}

#[derive(Debug)]
pub enum PolicyError {
    Io(io::Error),
    Parse(String),
    Invalid(String),
    UnsupportedPlatform,
    Permission(String),
}

impl fmt::Display for PolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "policy I/O error: {error}"),
            Self::Parse(message) => write!(f, "policy parse error: {message}"),
            Self::Invalid(message) => write!(f, "invalid policy: {message}"),
            Self::UnsupportedPlatform => {
                f.write_str("root-owned policy trust validation requires Linux")
            }
            Self::Permission(message) => write!(f, "policy trust error: {message}"),
        }
    }
}

impl std::error::Error for PolicyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for PolicyError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

pub type PolicyResult<T> = Result<T, PolicyError>;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GlobalPolicy {
    pub policy_version: u32,
    pub default_effect: PolicyEffect,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActionRegistry {
    pub policy_version: u32,
    pub actions: Vec<ActionDefinition>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActionDefinition {
    pub id: String,
    pub label: String,
    pub class: PolicyEnforcementClass,
    #[serde(default)]
    pub resource_kind: Option<String>,
    #[serde(default)]
    pub resource_required: bool,
    #[serde(default)]
    pub root_equivalent: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BundleDefinition {
    pub policy_version: u32,
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub broad_default: bool,
    #[serde(default)]
    pub grants: Vec<GrantDefinition>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GrantDefinition {
    pub action: String,
    pub effect: PolicyEffect,
    #[serde(default)]
    pub resources: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BundleSelection {
    pub id: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SubjectPolicy {
    pub policy_version: u32,
    pub uid: u32,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub bundles: Vec<BundleSelection>,
    #[serde(default)]
    pub grants: Vec<GrantDefinition>,
}

impl SubjectPolicy {
    #[must_use]
    pub fn empty(uid: u32) -> Self {
        Self {
            policy_version: POLICY_VERSION,
            uid,
            label: None,
            bundles: Vec::new(),
            grants: Vec::new(),
        }
    }
}

pub(crate) fn validate_identifier(value: &str, field: &str) -> PolicyResult<()> {
    if value.is_empty()
        || value.len() > MAX_ID_BYTES
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-' | b'_')
        })
    {
        return Err(PolicyError::Invalid(format!("{field} is invalid")));
    }
    Ok(())
}

pub(crate) fn validate_resource(value: &str) -> PolicyResult<()> {
    if value.is_empty() || value.len() > MAX_RESOURCE_BYTES || value.contains(['\0', '\n', '\r']) {
        return Err(PolicyError::Invalid(
            "resource identifier is invalid".into(),
        ));
    }
    Ok(())
}

pub(crate) fn action_map(
    registry: &ActionRegistry,
) -> PolicyResult<BTreeMap<String, ActionDefinition>> {
    if registry.policy_version != POLICY_VERSION || registry.actions.len() > MAX_ACTIONS {
        return Err(PolicyError::Invalid(
            "action registry version/count is invalid".into(),
        ));
    }
    let mut map = BTreeMap::new();
    for action in &registry.actions {
        validate_identifier(&action.id, "action id")?;
        if action.label.is_empty() || action.label.len() > MAX_LABEL_BYTES {
            return Err(PolicyError::Invalid("action label is invalid".into()));
        }
        if action.resource_required && action.resource_kind.is_none() {
            return Err(PolicyError::Invalid(
                "resource-required action lacks resource kind".into(),
            ));
        }
        if action.root_equivalent != (action.class == PolicyEnforcementClass::RootEquivalent) {
            return Err(PolicyError::Invalid(
                "root-equivalent action classification is inconsistent".into(),
            ));
        }
        if map.insert(action.id.clone(), action.clone()).is_some() {
            return Err(PolicyError::Invalid("duplicate action id".into()));
        }
    }
    Ok(map)
}
