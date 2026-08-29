use crate::{
    MAX_OPERATIONS_PER_PROVIDER, MAX_PROVIDER_DEFINITIONS, MAX_REQUEST_BYTES, MAX_RESPONSE_BYTES,
    MAX_TIMEOUT_MS, ProviderError, ProviderErrorCode, validate_operation_id, validate_provider_id,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use url::Url;

const MAX_DEFINITION_FILE_BYTES: u64 = 256 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DefinitionTrust {
    RootOwnedSystem,
    PretrustedFixture,
}

#[derive(Clone, Debug)]
pub struct DefinitionPaths {
    pub directory: PathBuf,
}

impl DefinitionPaths {
    #[must_use]
    pub fn canonical() -> Self {
        Self {
            directory: crate::CANONICAL_PROVIDER_DEFINITIONS_DIR.into(),
        }
    }
}

#[derive(Debug)]
pub enum DefinitionError {
    Io(std::io::Error),
    Parse(String),
    Invalid(String),
    UnsupportedPlatform,
    Permission(String),
}

impl std::fmt::Display for DefinitionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "provider-definition I/O error: {error}"),
            Self::Parse(message) => write!(f, "provider-definition parse error: {message}"),
            Self::Invalid(message) => write!(f, "invalid provider definition: {message}"),
            Self::UnsupportedPlatform => {
                f.write_str("root-owned provider-definition validation requires Linux")
            }
            Self::Permission(message) => write!(f, "provider-definition trust error: {message}"),
        }
    }
}
impl std::error::Error for DefinitionError {}
impl From<std::io::Error> for DefinitionError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}
pub type DefinitionResult<T> = Result<T, DefinitionError>;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderDefinition {
    pub schema_version: u32,
    pub provider_id: String,
    pub origin: String,
    pub authentication: AuthenticationDefinition,
    pub limits: DefinitionLimits,
    pub operations: BTreeMap<String, OperationDefinition>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AuthenticationDefinition {
    pub kind: String,
    pub header: String,
    pub prefix: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DefinitionLimits {
    pub max_request_bytes: usize,
    pub max_response_bytes: usize,
    pub timeout_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OperationDefinition {
    pub method: String,
    pub path: String,
    pub streaming: bool,
}

#[derive(Clone, Debug)]
pub struct DefinitionCatalog {
    providers: BTreeMap<String, ProviderDefinition>,
}

impl DefinitionCatalog {
    pub fn load(paths: &DefinitionPaths, trust: DefinitionTrust) -> DefinitionResult<Self> {
        validate_trust(&paths.directory, trust)?;
        let mut files = Vec::new();
        for entry in fs::read_dir(&paths.directory)? {
            let path = entry?.path();
            if path.extension().and_then(|value| value.to_str()) == Some("toml") {
                files.push(path);
            }
        }
        files.sort();
        if files.len() > MAX_PROVIDER_DEFINITIONS {
            return Err(DefinitionError::Invalid(
                "too many provider definitions".into(),
            ));
        }
        let mut providers = BTreeMap::new();
        for path in files {
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink()
                || !metadata.is_file()
                || metadata.len() > MAX_DEFINITION_FILE_BYTES
            {
                return Err(DefinitionError::Invalid(
                    "provider definition is not a bounded regular file".into(),
                ));
            }
            let text = fs::read_to_string(&path)?;
            let definition: ProviderDefinition = toml::from_str(&text).map_err(|_| {
                DefinitionError::Parse(format!("{} does not match schema v1", path.display()))
            })?;
            validate_definition(&definition)?;
            let stem = path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or_default();
            if stem != definition.provider_id {
                return Err(DefinitionError::Invalid(
                    "provider filename does not match provider_id".into(),
                ));
            }
            if providers
                .insert(definition.provider_id.clone(), definition)
                .is_some()
            {
                return Err(DefinitionError::Invalid("duplicate provider id".into()));
            }
        }
        Ok(Self { providers })
    }

    pub fn from_definitions(definitions: Vec<ProviderDefinition>) -> DefinitionResult<Self> {
        if definitions.len() > MAX_PROVIDER_DEFINITIONS {
            return Err(DefinitionError::Invalid(
                "too many provider definitions".into(),
            ));
        }
        let mut providers = BTreeMap::new();
        for definition in definitions {
            validate_definition(&definition)?;
            if providers
                .insert(definition.provider_id.clone(), definition)
                .is_some()
            {
                return Err(DefinitionError::Invalid("duplicate provider id".into()));
            }
        }
        Ok(Self { providers })
    }

    #[must_use]
    pub fn get(&self, provider_id: &str) -> Option<&ProviderDefinition> {
        self.providers.get(provider_id)
    }
    #[must_use]
    pub fn len(&self) -> usize {
        self.providers.len()
    }
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }
}

impl ProviderDefinition {
    pub fn operation(&self, operation: &str) -> Result<&OperationDefinition, ProviderError> {
        self.operations.get(operation).ok_or_else(|| {
            ProviderError::new(
                ProviderErrorCode::OperationNotAllowed,
                "named provider operation is not allowed",
            )
        })
    }

    pub fn operation_url(&self, operation: &str) -> Result<Url, ProviderError> {
        let op = self.operation(operation)?;
        let base = Url::parse(&self.origin).map_err(|_| {
            ProviderError::new(
                ProviderErrorCode::ProviderDefinitionInvalid,
                "provider origin is invalid",
            )
        })?;
        base.join(&op.path).map_err(|_| {
            ProviderError::new(
                ProviderErrorCode::ProviderDefinitionInvalid,
                "provider operation path is invalid",
            )
        })
    }
}

fn validate_definition(definition: &ProviderDefinition) -> DefinitionResult<()> {
    if definition.schema_version != 1 {
        return Err(DefinitionError::Invalid(
            "unsupported provider schema version".into(),
        ));
    }
    validate_provider_id(&definition.provider_id)
        .map_err(|_| DefinitionError::Invalid("provider id is invalid".into()))?;
    let origin = Url::parse(&definition.origin)
        .map_err(|_| DefinitionError::Invalid("provider origin is invalid".into()))?;
    if origin.scheme() != "https"
        || origin.host_str().is_none()
        || origin.username() != ""
        || origin.password().is_some()
        || origin.query().is_some()
        || origin.fragment().is_some()
        || origin.path() != "/"
    {
        return Err(DefinitionError::Invalid(
            "provider origin must be a bare verified HTTPS origin".into(),
        ));
    }
    if definition.authentication.kind != "bearer"
        || !definition
            .authentication
            .header
            .eq_ignore_ascii_case("authorization")
        || definition.authentication.prefix.is_empty()
        || definition.authentication.prefix.len() > 64
        || definition
            .authentication
            .prefix
            .contains(['\0', '\n', '\r'])
    {
        return Err(DefinitionError::Invalid(
            "first provider authentication must be bounded Authorization bearer placement".into(),
        ));
    }
    if definition.limits.max_request_bytes == 0
        || definition.limits.max_request_bytes > MAX_REQUEST_BYTES
        || definition.limits.max_response_bytes == 0
        || definition.limits.max_response_bytes > MAX_RESPONSE_BYTES
        || definition.limits.timeout_ms == 0
        || definition.limits.timeout_ms > MAX_TIMEOUT_MS
    {
        return Err(DefinitionError::Invalid(
            "provider limits exceed first-contract ceilings".into(),
        ));
    }
    if definition.operations.is_empty() || definition.operations.len() > MAX_OPERATIONS_PER_PROVIDER
    {
        return Err(DefinitionError::Invalid(
            "provider operation count is invalid".into(),
        ));
    }
    for (id, operation) in &definition.operations {
        validate_operation_id(id)
            .map_err(|_| DefinitionError::Invalid("operation id is invalid".into()))?;
        if !matches!(operation.method.as_str(), "GET" | "POST")
            || operation.streaming
            || !operation.path.starts_with('/')
            || operation.path.starts_with("//")
            || operation.path.contains(['\0', '\n', '\r', '?', '#'])
            || operation.path.contains("://")
        {
            return Err(DefinitionError::Invalid(
                "operation method/path/streaming shape is invalid".into(),
            ));
        }
    }
    Ok(())
}

fn validate_trust(directory: &Path, trust: DefinitionTrust) -> DefinitionResult<()> {
    if trust == DefinitionTrust::PretrustedFixture {
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        for trusted_directory in [directory.parent(), Some(directory)].into_iter().flatten() {
            let metadata = fs::symlink_metadata(trusted_directory)?;
            if metadata.file_type().is_symlink()
                || !metadata.is_dir()
                || metadata.uid() != 0
                || metadata.permissions().mode() & 0o022 != 0
            {
                return Err(DefinitionError::Permission(format!(
                    "{} is not trusted root-owned provider-definition material",
                    trusted_directory.display()
                )));
            }
        }
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            if path.extension().and_then(|value| value.to_str()) != Some("toml") {
                continue;
            }
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink()
                || !metadata.is_file()
                || metadata.uid() != 0
                || metadata.permissions().mode() & 0o022 != 0
            {
                return Err(DefinitionError::Permission(format!(
                    "{} is not trusted root-owned provider material",
                    path.display()
                )));
            }
        }
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = directory;
        Err(DefinitionError::UnsupportedPlatform)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn openai() -> ProviderDefinition {
        ProviderDefinition {
            schema_version: 1,
            provider_id: "openai".into(),
            origin: "https://api.openai.com".into(),
            authentication: AuthenticationDefinition {
                kind: "bearer".into(),
                header: "Authorization".into(),
                prefix: "Bearer ".into(),
            },
            limits: DefinitionLimits {
                max_request_bytes: MAX_REQUEST_BYTES,
                max_response_bytes: MAX_RESPONSE_BYTES,
                timeout_ms: MAX_TIMEOUT_MS,
            },
            operations: BTreeMap::from([(
                "openai.responses.create".into(),
                OperationDefinition {
                    method: "POST".into(),
                    path: "/v1/responses".into(),
                    streaming: false,
                },
            )]),
        }
    }

    #[test]
    fn shipped_openai_definition_matches_reference_contract() {
        const OPENAI: &str =
            include_str!("../../../integrations/protected-api/providers/openai.toml");
        let definition: ProviderDefinition = toml::from_str(OPENAI).unwrap();
        let catalog = DefinitionCatalog::from_definitions(vec![definition]).unwrap();
        assert_eq!(
            catalog
                .get("openai")
                .unwrap()
                .operation_url("openai.responses.create")
                .unwrap()
                .as_str(),
            "https://api.openai.com/v1/responses"
        );
    }

    #[test]
    fn reference_definition_is_fixed_https_and_bounded() {
        let catalog = DefinitionCatalog::from_definitions(vec![openai()]).unwrap();
        let definition = catalog.get("openai").unwrap();
        assert_eq!(
            definition
                .operation_url("openai.responses.create")
                .unwrap()
                .as_str(),
            "https://api.openai.com/v1/responses"
        );
    }

    #[test]
    fn insecure_or_unbounded_definitions_fail_closed() {
        for mutate in [0, 1, 2, 3] {
            let mut value = openai();
            match mutate {
                0 => value.origin = "http://attacker.invalid".into(),
                1 => value.limits.max_request_bytes = MAX_REQUEST_BYTES + 1,
                2 => {
                    value
                        .operations
                        .get_mut("openai.responses.create")
                        .unwrap()
                        .path = "https://attacker.invalid/steal".into()
                }
                _ => {
                    value
                        .operations
                        .get_mut("openai.responses.create")
                        .unwrap()
                        .streaming = true
                }
            }
            assert!(DefinitionCatalog::from_definitions(vec![value]).is_err());
        }
    }
}
