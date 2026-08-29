//! PortusOS integration contract for the independent PortusBrowser provider.
//!
//! This crate intentionally does not proxy browser commands or copy browser
//! domain state into PortusOS. It validates the first-ISO compatibility set,
//! maps bounded structured CLI health/session output into generic provider
//! state, and prepares the Chromium native-messaging registration inputs used
//! later by the installed-system packaging layer.

use portus_protocol::{
    HealthComponentType, HealthObservation, HealthReasonCode, HealthState, Principal,
    ProviderRegistrationId, ProviderResourceId, ProviderResourceRef, RecoveryDisposition,
    ResourceType,
};
use portus_provider::{HealthIntegrationKind, LifecycleOwner, ProviderManifest, ResourceLifetime};
use portus_state::{
    PortusState, ProviderCapabilityRuntimeSpec, ProviderResourceRuntimeSpec,
    ProviderRuntimeStatusSpec, StateError,
};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, HashSet},
    fmt,
};

pub const INTEGRATION_VERSION: u32 = 1;
pub const MAX_BROWSER_SESSIONS: usize = 128;
pub const MAX_FIELD_BYTES: usize = 512;
pub const PORTUS_BROWSER_PROVIDER_TYPE: &str = "portus-browser";
pub const PORTUS_BROWSER_CAPABILITY: &str = "browser.control";
pub const PORTUS_BROWSER_RESOURCE_TYPE: &str = "browser-session";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum SourcePinState {
    PendingCleanSourceFreeze,
    Pinned,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LifecycleContract {
    pub owner: String,
    pub broker_start: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ReferenceBrowserContract {
    pub family: String,
    pub xdg_native_messaging_relative_dir: String,
    pub native_host_name: String,
    pub extension_id_source: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PortusBrowserContract {
    pub integration_version: u32,
    pub provider_type: String,
    pub software_version: String,
    pub protocol_version: String,
    pub source_repository: String,
    pub source_pin_state: SourcePinState,
    #[serde(default)]
    pub source_revision: Option<String>,
    pub cli_contract_version: u32,
    pub capability_id: String,
    pub capability_contract_version: u32,
    pub resource_type: String,
    pub cli_executable: String,
    pub broker_executable: String,
    pub native_host_executable: String,
    pub skill_source: String,
    pub lifecycle: LifecycleContract,
    pub reference_browser: ReferenceBrowserContract,
}

#[derive(Debug)]
pub enum IntegrationError {
    Parse(String),
    Invalid(&'static str),
    Manifest(&'static str),
    State(StateError),
}

impl fmt::Display for IntegrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(message) => write!(f, "PortusBrowser integration parse error: {message}"),
            Self::Invalid(message) => write!(f, "invalid PortusBrowser integration: {message}"),
            Self::Manifest(message) => write!(f, "PortusBrowser manifest mismatch: {message}"),
            Self::State(error) => write!(f, "PortusBrowser state integration error: {error}"),
        }
    }
}

impl std::error::Error for IntegrationError {}

impl From<StateError> for IntegrationError {
    fn from(value: StateError) -> Self {
        Self::State(value)
    }
}

pub type IntegrationResult<T> = Result<T, IntegrationError>;

impl PortusBrowserContract {
    pub fn parse(contents: &str) -> IntegrationResult<Self> {
        let contract: Self =
            toml::from_str(contents).map_err(|error| IntegrationError::Parse(error.to_string()))?;
        contract.validate()?;
        Ok(contract)
    }

    pub fn validate(&self) -> IntegrationResult<()> {
        if self.integration_version != INTEGRATION_VERSION {
            return Err(IntegrationError::Invalid("unsupported integration version"));
        }
        if self.provider_type != PORTUS_BROWSER_PROVIDER_TYPE
            || self.capability_id != PORTUS_BROWSER_CAPABILITY
            || self.resource_type != PORTUS_BROWSER_RESOURCE_TYPE
        {
            return Err(IntegrationError::Invalid(
                "provider/capability/resource identity changed",
            ));
        }
        for value in [
            &self.software_version,
            &self.protocol_version,
            &self.source_repository,
            &self.cli_executable,
            &self.broker_executable,
            &self.native_host_executable,
            &self.skill_source,
        ] {
            validate_text(value)?;
        }
        if !self.source_repository.starts_with("https://")
            || !self.source_repository.ends_with("/portus-browser.git")
        {
            return Err(IntegrationError::Invalid(
                "source repository must be the HTTPS PortusBrowser repository",
            ));
        }
        if self.cli_contract_version == 0 || self.capability_contract_version == 0 {
            return Err(IntegrationError::Invalid(
                "contract versions must be nonzero",
            ));
        }
        for executable in [
            &self.cli_executable,
            &self.broker_executable,
            &self.native_host_executable,
        ] {
            if !is_absolute_linux_path(executable) {
                return Err(IntegrationError::Invalid(
                    "installed executable paths must be absolute Linux paths",
                ));
            }
        }
        match (self.source_pin_state, self.source_revision.as_deref()) {
            (SourcePinState::PendingCleanSourceFreeze, None) => {}
            (SourcePinState::Pinned, Some(revision)) if is_git_revision(revision) => {}
            (SourcePinState::PendingCleanSourceFreeze, Some(_)) => {
                return Err(IntegrationError::Invalid(
                    "pending source freeze cannot claim a revision pin",
                ));
            }
            (SourcePinState::Pinned, _) => {
                return Err(IntegrationError::Invalid(
                    "pinned source requires a 40-character Git revision",
                ));
            }
        }
        if self.lifecycle.owner != "provider-owned"
            || self.lifecycle.broker_start != "native-host-on-demand"
        {
            return Err(IntegrationError::Invalid(
                "first-ISO Broker lifecycle must remain provider-owned on-demand",
            ));
        }
        if self.reference_browser.family != "chromium"
            || self.reference_browser.xdg_native_messaging_relative_dir
                != "chromium/NativeMessagingHosts"
            || self.reference_browser.native_host_name != "com.portus.browser"
            || self.reference_browser.extension_id_source != "installed-extension"
        {
            return Err(IntegrationError::Invalid(
                "Chromium reference-browser contract changed",
            ));
        }
        Ok(())
    }

    #[must_use]
    pub fn release_pin_ready(&self) -> bool {
        self.source_pin_state == SourcePinState::Pinned && self.source_revision.is_some()
    }

    pub fn validate_provider_manifest(&self, manifest: &ProviderManifest) -> IntegrationResult<()> {
        if manifest.provider.provider_type != self.provider_type
            || manifest.provider.software_version != self.software_version
            || manifest.lifecycle.owner != LifecycleOwner::ProviderOwned
            || manifest.health.kind != HealthIntegrationKind::StructuredCli
            || manifest.health.reference.as_deref() != Some("cli")
        {
            return Err(IntegrationError::Manifest(
                "provider identity/version/lifecycle/health contract differs",
            ));
        }
        let Some(cli) = manifest.interfaces.iter().find(|item| item.id == "cli") else {
            return Err(IntegrationError::Manifest("CLI interface is missing"));
        };
        if cli.contract_version != self.cli_contract_version
            || cli.executable.as_deref() != Some(self.cli_executable.as_str())
            || !cli.structured_output
        {
            return Err(IntegrationError::Manifest(
                "CLI interface does not match integration contract",
            ));
        }
        let Some(capability) = manifest
            .capabilities
            .iter()
            .find(|item| item.id == self.capability_id)
        else {
            return Err(IntegrationError::Manifest(
                "browser.control capability is missing",
            ));
        };
        if capability.contract_version != self.capability_contract_version
            || capability.interfaces != ["cli"]
        {
            return Err(IntegrationError::Manifest(
                "browser.control capability contract differs",
            ));
        }
        let Some(resource) = manifest
            .resources
            .iter()
            .find(|item| item.resource_type == self.resource_type)
        else {
            return Err(IntegrationError::Manifest(
                "browser-session resource type is missing",
            ));
        };
        if resource.lifetime != ResourceLifetime::Session
            || !manifest
                .skills
                .iter()
                .any(|skill| skill == "portus-browser")
        {
            return Err(IntegrationError::Manifest(
                "resource lifetime or provider skill differs",
            ));
        }
        Ok(())
    }

    pub fn chromium_native_messaging_spec(
        &self,
        xdg_config_home: &str,
        extension_id: &str,
    ) -> IntegrationResult<NativeMessagingSpec> {
        if !is_absolute_linux_path(xdg_config_home) {
            return Err(IntegrationError::Invalid(
                "XDG_CONFIG_HOME must be an absolute Linux path",
            ));
        }
        if !is_extension_id(extension_id) {
            return Err(IntegrationError::Invalid(
                "Chromium extension id is invalid",
            ));
        }
        Ok(NativeMessagingSpec {
            manifest_directory: format!(
                "{}/{}",
                xdg_config_home.trim_end_matches('/'),
                self.reference_browser.xdg_native_messaging_relative_dir
            ),
            host_name: self.reference_browser.native_host_name.clone(),
            native_host_executable: self.native_host_executable.clone(),
            allowed_origin: format!("chrome-extension://{extension_id}/"),
        })
    }
    #[must_use]
    pub fn repair_plan(&self) -> [RepairStep; 5] {
        [
            RepairStep::ProbeBroker,
            RepairStep::ProbeBrowserSessions,
            RepairStep::ReconcileProviderRegistry,
            RepairStep::ReRegisterNativeMessaging,
            RepairStep::Reprobe,
        ]
    }

    pub fn project_runtime(
        &self,
        provider_id: ProviderRegistrationId,
        owner: Principal,
        broker_probe: ProbeInput<'_>,
        browsers_probe: ProbeInput<'_>,
        now_ms: i64,
    ) -> PortusBrowserProjection {
        let broker = match broker_probe {
            ProbeInput::Unavailable => {
                return projection(
                    self,
                    provider_id,
                    owner,
                    ProviderRuntimeState::Unavailable,
                    "broker_unavailable",
                    Vec::new(),
                    now_ms,
                );
            }
            ProbeInput::Output(value) => match parse_broker_status(value) {
                Ok(value) => value,
                Err(_) => {
                    return projection(
                        self,
                        provider_id,
                        owner,
                        ProviderRuntimeState::DegradedUnknownCompatibility,
                        "status_unavailable",
                        Vec::new(),
                        now_ms,
                    );
                }
            },
        };
        if !broker.running {
            return projection(
                self,
                provider_id,
                owner,
                ProviderRuntimeState::Unavailable,
                "broker_not_running",
                Vec::new(),
                now_ms,
            );
        }
        if broker.protocol_version != self.protocol_version {
            return projection(
                self,
                provider_id,
                owner,
                ProviderRuntimeState::Incompatible,
                "protocol_incompatible",
                Vec::new(),
                now_ms,
            );
        }

        let browsers = match browsers_probe {
            ProbeInput::Unavailable => {
                return projection(
                    self,
                    provider_id,
                    owner,
                    ProviderRuntimeState::DegradedCompatible,
                    "browser_list_unavailable",
                    Vec::new(),
                    now_ms,
                );
            }
            ProbeInput::Output(value) => match parse_browser_list(value) {
                Ok(value) => value,
                Err(_) => {
                    return projection(
                        self,
                        provider_id,
                        owner,
                        ProviderRuntimeState::DegradedCompatible,
                        "browser_list_invalid",
                        Vec::new(),
                        now_ms,
                    );
                }
            },
        };

        let degraded = browsers
            .iter()
            .any(|browser| browser.status != "available" || browser.bridge_status != "connected");
        let resources = browsers
            .into_iter()
            .filter_map(|browser| {
                let resource_id = ProviderResourceId::new(browser.browser_id).ok()?;
                let resource_type = ResourceType::new(self.resource_type.clone()).ok()?;
                let available =
                    browser.status == "available" && browser.bridge_status == "connected";
                let reference = ProviderResourceRef::new(provider_id, resource_type, resource_id)
                    .with_generation(browser.connected_at);
                Some(ProviderResourceRuntimeSpec {
                    reference,
                    availability_state: if available {
                        "available"
                    } else {
                        "unavailable"
                    }
                    .into(),
                })
            })
            .collect::<Vec<_>>();
        projection(
            self,
            provider_id,
            owner,
            if degraded {
                ProviderRuntimeState::DegradedCompatible
            } else {
                ProviderRuntimeState::HealthyCompatible
            },
            if degraded {
                "provider_degraded"
            } else {
                "ready"
            },
            resources,
            now_ms,
        )
    }

    pub fn apply_projection(
        &self,
        state: &mut PortusState,
        provider_id: &ProviderRegistrationId,
        owner: Principal,
        projection: &PortusBrowserProjection,
    ) -> IntegrationResult<()> {
        state.update_provider_runtime_status(
            provider_id,
            &projection.runtime_status,
            projection.observed_at_ms,
        )?;
        state.reconcile_provider_resource_refs(
            provider_id,
            &self.resource_type,
            Some(owner),
            &projection.resources,
            projection.observed_at_ms,
        )?;
        state.record_health_observation(&projection.health_observation)?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProbeInput<'a> {
    Output(&'a str),
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepairStep {
    ProbeBroker,
    ProbeBrowserSessions,
    ReconcileProviderRegistry,
    ReRegisterNativeMessaging,
    Reprobe,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeMessagingSpec {
    pub manifest_directory: String,
    pub host_name: String,
    pub native_host_executable: String,
    pub allowed_origin: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortusBrowserProjection {
    pub runtime_status: ProviderRuntimeStatusSpec,
    pub resources: Vec<ProviderResourceRuntimeSpec>,
    pub health_observation: HealthObservation,
    pub observed_at_ms: i64,
}

#[derive(Clone, Copy)]
enum ProviderRuntimeState {
    HealthyCompatible,
    DegradedCompatible,
    DegradedUnknownCompatibility,
    Unavailable,
    Incompatible,
}

fn projection(
    contract: &PortusBrowserContract,
    provider_id: ProviderRegistrationId,
    owner: Principal,
    state: ProviderRuntimeState,
    reason: &str,
    resources: Vec<ProviderResourceRuntimeSpec>,
    now_ms: i64,
) -> PortusBrowserProjection {
    let (compatibility, health, capability, health_reason, disposition) = match state {
        ProviderRuntimeState::HealthyCompatible => (
            "compatible",
            "healthy",
            "available",
            HealthReasonCode::Ready,
            RecoveryDisposition::Observe,
        ),
        ProviderRuntimeState::DegradedCompatible => (
            "compatible",
            "degraded",
            "degraded",
            HealthReasonCode::ProviderDegraded,
            RecoveryDisposition::Reconcile,
        ),
        ProviderRuntimeState::DegradedUnknownCompatibility => (
            "unknown",
            "degraded",
            "degraded",
            HealthReasonCode::StatusUnavailable,
            RecoveryDisposition::Reconcile,
        ),
        ProviderRuntimeState::Unavailable => (
            "unknown",
            "unavailable",
            "unavailable",
            HealthReasonCode::ProviderUnavailable,
            RecoveryDisposition::Reconcile,
        ),
        ProviderRuntimeState::Incompatible => (
            "incompatible",
            "unavailable",
            "unavailable",
            HealthReasonCode::Incompatible,
            RecoveryDisposition::AdministratorRequired,
        ),
    };
    let mut safe_details = BTreeMap::new();
    safe_details.insert("software_version".into(), contract.software_version.clone());
    safe_details.insert("protocol_version".into(), contract.protocol_version.clone());
    safe_details.insert("browser_session_count".into(), resources.len().to_string());
    PortusBrowserProjection {
        runtime_status: ProviderRuntimeStatusSpec {
            compatibility_state: compatibility.into(),
            health_state: health.into(),
            health_reason: Some(reason.into()),
            capabilities: vec![ProviderCapabilityRuntimeSpec {
                capability_id: contract.capability_id.clone(),
                availability_state: capability.into(),
                reason_code: (reason != "ready").then(|| reason.into()),
            }],
        },
        resources,
        health_observation: HealthObservation {
            component_ref: format!("provider:{provider_id}"),
            component_type: HealthComponentType::Provider,
            owner: Some(owner),
            health_state: match health {
                "healthy" => HealthState::Healthy,
                "degraded" => HealthState::Degraded,
                "unavailable" => HealthState::Unavailable,
                _ => HealthState::Unknown,
            },
            reason_code: health_reason,
            summary: match state {
                ProviderRuntimeState::HealthyCompatible => {
                    "PortusBrowser provider is healthy".into()
                }
                ProviderRuntimeState::DegradedCompatible
                | ProviderRuntimeState::DegradedUnknownCompatibility => {
                    "PortusBrowser provider is degraded".into()
                }
                ProviderRuntimeState::Unavailable => "PortusBrowser provider is unavailable".into(),
                ProviderRuntimeState::Incompatible => {
                    "PortusBrowser protocol is incompatible".into()
                }
            },
            source: "portus-browser-integration".into(),
            observed_at_ms: now_ms,
            source_generation: None,
            last_healthy_at_ms: (health == "healthy").then_some(now_ms),
            recovery_disposition: disposition,
            recovery_attempt_count: 0,
            safe_details,
        },
        observed_at_ms: now_ms,
    }
}

#[derive(Deserialize)]
struct BrokerStatusEnvelope {
    ok: bool,
    #[serde(default)]
    broker: Option<BrokerStatus>,
}

#[derive(Deserialize)]
struct BrokerStatus {
    running: bool,
    #[serde(rename = "protocolVersion")]
    protocol_version: String,
}

fn parse_broker_status(value: &str) -> IntegrationResult<BrokerStatus> {
    let envelope: BrokerStatusEnvelope =
        serde_json::from_str(value).map_err(|error| IntegrationError::Parse(error.to_string()))?;
    if !envelope.ok {
        return Err(IntegrationError::Invalid(
            "broker status command did not succeed",
        ));
    }
    let broker = envelope.broker.ok_or(IntegrationError::Invalid(
        "broker status payload is missing",
    ))?;
    validate_text(&broker.protocol_version)?;
    Ok(broker)
}

#[derive(Deserialize)]
struct BrowserListEnvelope {
    ok: bool,
    #[serde(default)]
    browsers: Vec<BrowserSession>,
}

#[derive(Deserialize)]
struct BrowserSession {
    #[serde(rename = "browserId")]
    browser_id: String,
    #[serde(rename = "connectedAt")]
    connected_at: String,
    #[serde(rename = "extensionVersion")]
    extension_version: String,
    #[serde(rename = "bridgeStatus")]
    bridge_status: String,
    status: String,
}

fn parse_browser_list(value: &str) -> IntegrationResult<Vec<BrowserSession>> {
    let envelope: BrowserListEnvelope =
        serde_json::from_str(value).map_err(|error| IntegrationError::Parse(error.to_string()))?;
    if !envelope.ok || envelope.browsers.len() > MAX_BROWSER_SESSIONS {
        return Err(IntegrationError::Invalid(
            "browser list command failed or exceeded bounds",
        ));
    }
    let mut identities = HashSet::new();
    for browser in &envelope.browsers {
        if !is_browser_id(&browser.browser_id)
            || !matches!(
                browser.bridge_status.as_str(),
                "connected" | "disconnecting" | "disconnected" | "error"
            )
            || !matches!(
                browser.status.as_str(),
                "available" | "expired" | "unavailable"
            )
        {
            return Err(IntegrationError::Invalid(
                "browser session identity or state is invalid",
            ));
        }
        validate_text(&browser.connected_at)?;
        validate_text(&browser.extension_version)?;
        let key = format!("{}:{}", browser.browser_id, browser.connected_at);
        if !identities.insert(key) {
            return Err(IntegrationError::Invalid(
                "browser list contains duplicate session identity",
            ));
        }
    }
    Ok(envelope.browsers)
}

fn validate_text(value: &str) -> IntegrationResult<()> {
    if value.is_empty() || value.len() > MAX_FIELD_BYTES || value.contains(['\0', '\n', '\r']) {
        return Err(IntegrationError::Invalid(
            "text field is empty, multiline, or oversized",
        ));
    }
    Ok(())
}

fn is_absolute_linux_path(value: &str) -> bool {
    value.starts_with('/')
        && value.len() <= MAX_FIELD_BYTES
        && !value.contains(['\\', '\0', '\n', '\r'])
        && !value
            .split('/')
            .any(|segment| matches!(segment, "." | ".."))
}

fn is_git_revision(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn is_extension_id(value: &str) -> bool {
    value.len() == 32 && value.bytes().all(|byte| matches!(byte, b'a'..=b'p'))
}

fn is_browser_id(value: &str) -> bool {
    value.strip_prefix("br_").is_some_and(|suffix| {
        !suffix.is_empty()
            && suffix.len() <= 96
            && suffix
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use portus_provider::ProviderManifest;
    use portus_state::PortusState;
    use std::fs;

    const CONTRACT: &str = include_str!("../../../integrations/portus-browser/integration.toml");
    const MANIFEST: &str = include_str!("../../../integrations/manifests/portus-browser.toml");

    fn contract() -> PortusBrowserContract {
        PortusBrowserContract::parse(CONTRACT).unwrap()
    }

    fn broker(protocol: &str) -> String {
        format!(
            r#"{{"ok":true,"broker":{{"running":true,"pipePath":"/run/user/1000/portus-browser.sock","pipeName":"portus-browser-broker","startedAt":"2026-08-27T00:00:00Z","protocolVersion":"{protocol}"}}}}"#
        )
    }

    fn browsers(items: &str) -> String {
        format!(r#"{{"ok":true,"browsers":[{items}]}}"#)
    }

    fn browser(id: &str, connected_at: &str, bridge: &str, status: &str) -> String {
        format!(
            r#"{{"browserId":"{id}","browserName":"Chrome","extensionVersion":"0.1.0","connectedAt":"{connected_at}","lastHeartbeat":"{connected_at}","capabilities":["tabs","snapshots","actions"],"bridgeStatus":"{bridge}","status":"{status}"}}"#
        )
    }

    #[test]
    fn shipped_contract_is_valid_and_pins_the_verified_clean_source_revision() {
        let contract = contract();
        assert_eq!(contract.protocol_version, "2");
        assert_eq!(contract.software_version, "0.1.0");
        assert!(contract.release_pin_ready());
        assert_eq!(
            contract.source_revision.as_deref(),
            Some("c263c3997b4e6f2f7df5922e062a9e949e22f755")
        );
    }

    #[test]
    fn shipped_provider_manifest_matches_the_integration_contract() {
        let contract = contract();
        let manifest = ProviderManifest::parse("portus-browser.toml", MANIFEST).unwrap();
        contract.validate_provider_manifest(&manifest).unwrap();
    }

    #[test]
    fn healthy_broker_with_zero_sessions_is_healthy_not_unavailable() {
        let contract = contract();
        let broker_output = broker("2");
        let browser_output = browsers("");
        let projection = contract.project_runtime(
            ProviderRegistrationId::new(),
            Principal::new(1000, 1000),
            ProbeInput::Output(&broker_output),
            ProbeInput::Output(&browser_output),
            10,
        );
        assert_eq!(projection.runtime_status.compatibility_state, "compatible");
        assert_eq!(projection.runtime_status.health_state, "healthy");
        assert!(projection.resources.is_empty());
        assert_eq!(
            projection.health_observation.health_state,
            HealthState::Healthy
        );
    }

    #[test]
    fn unavailable_broker_is_distinct_from_healthy_zero_session_provider() {
        let contract = contract();
        let projection = contract.project_runtime(
            ProviderRegistrationId::new(),
            Principal::new(1000, 1000),
            ProbeInput::Unavailable,
            ProbeInput::Unavailable,
            10,
        );
        assert_eq!(projection.runtime_status.health_state, "unavailable");
        assert_eq!(projection.runtime_status.compatibility_state, "unknown");
        assert_eq!(
            projection.health_observation.reason_code,
            HealthReasonCode::ProviderUnavailable
        );
    }

    #[test]
    fn incompatible_protocol_fails_closed() {
        let contract = contract();
        let broker_output = broker("99");
        let browser_output = browsers("");
        let projection = contract.project_runtime(
            ProviderRegistrationId::new(),
            Principal::new(1000, 1000),
            ProbeInput::Output(&broker_output),
            ProbeInput::Output(&browser_output),
            10,
        );
        assert_eq!(
            projection.runtime_status.compatibility_state,
            "incompatible"
        );
        assert_eq!(projection.runtime_status.health_state, "unavailable");
        assert_eq!(
            projection.health_observation.recovery_disposition,
            RecoveryDisposition::AdministratorRequired
        );
    }

    #[test]
    fn browser_sessions_project_only_to_opaque_provider_resources() {
        let contract = contract();
        let items = browser(
            "br_000001",
            "2026-08-27T00:00:00Z",
            "connected",
            "available",
        );
        let broker_output = broker("2");
        let browser_output = browsers(&items);
        let projection = contract.project_runtime(
            ProviderRegistrationId::new(),
            Principal::new(1000, 1000),
            ProbeInput::Output(&broker_output),
            ProbeInput::Output(&browser_output),
            10,
        );
        assert_eq!(projection.resources.len(), 1);
        assert_eq!(
            projection.resources[0].reference.resource_type.as_str(),
            "browser-session"
        );
        let debug = format!("{:?}", projection.resources[0].reference);
        assert!(!debug.contains("br_000001"));
        let health_json = serde_json::to_string(&projection.health_observation).unwrap();
        assert!(!health_json.contains("br_000001"));
        assert!(!health_json.contains("url"));
        assert!(!health_json.contains("snapshot"));
        assert!(!health_json.contains("dom"));
    }

    #[test]
    fn degraded_bridge_degrades_provider_without_copying_browser_domain_state() {
        let contract = contract();
        let items = browser("br_000001", "2026-08-27T00:00:00Z", "error", "unavailable");
        let broker_output = broker("2");
        let browser_output = browsers(&items);
        let projection = contract.project_runtime(
            ProviderRegistrationId::new(),
            Principal::new(1000, 1000),
            ProbeInput::Output(&broker_output),
            ProbeInput::Output(&browser_output),
            10,
        );
        assert_eq!(projection.runtime_status.health_state, "degraded");
        assert_eq!(projection.resources[0].availability_state, "unavailable");
    }

    #[test]
    fn chromium_registration_uses_xdg_profile_and_explicit_extension_origin() {
        let contract = contract();
        let spec = contract
            .chromium_native_messaging_spec(
                "/home/master/.config",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            )
            .unwrap();
        assert_eq!(
            spec.manifest_directory,
            "/home/master/.config/chromium/NativeMessagingHosts"
        );
        assert_eq!(spec.host_name, "com.portus.browser");
        assert_eq!(spec.native_host_executable, "/usr/bin/portus-native-host");
        assert_eq!(
            spec.allowed_origin,
            "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/"
        );
    }

    #[test]
    fn runtime_projection_updates_generic_provider_state_and_stales_disappeared_sessions() {
        let dir = std::env::temp_dir().join(format!(
            "portus-browser-integration-{}",
            portus_protocol::TaskId::new()
        ));
        fs::create_dir_all(&dir).unwrap();
        let db = dir.join("portus.db");
        let mut state = PortusState::open(&db).unwrap();
        let contract = contract();
        let manifest = ProviderManifest::parse("portus-browser.toml", MANIFEST).unwrap();
        let registration = state
            .reconcile_provider_registration(
                &manifest
                    .to_system_registration_spec("portus-browser.toml".into())
                    .unwrap(),
                1,
            )
            .unwrap();
        let owner = Principal::new(1000, 1000);
        let first_items = browser(
            "br_000001",
            "2026-08-27T00:00:00Z",
            "connected",
            "available",
        );
        let broker_output = broker("2");
        let first_browser_output = browsers(&first_items);
        let first = contract.project_runtime(
            registration.provider_id,
            owner,
            ProbeInput::Output(&broker_output),
            ProbeInput::Output(&first_browser_output),
            10,
        );
        let first_ref = first.resources[0].reference.clone();
        contract
            .apply_projection(&mut state, &registration.provider_id, owner, &first)
            .unwrap();
        assert_eq!(
            state
                .provider_resource_availability(&first_ref)
                .unwrap()
                .as_deref(),
            Some("available")
        );

        let second_browser_output = browsers("");
        let second = contract.project_runtime(
            registration.provider_id,
            owner,
            ProbeInput::Output(&broker_output),
            ProbeInput::Output(&second_browser_output),
            20,
        );
        contract
            .apply_projection(&mut state, &registration.provider_id, owner, &second)
            .unwrap();
        assert_eq!(
            state
                .provider_resource_availability(&first_ref)
                .unwrap()
                .as_deref(),
            Some("stale")
        );
        let view = state
            .provider_visible_by_id(&registration.provider_id, owner)
            .unwrap()
            .unwrap();
        assert_eq!(view.registration.health_state, "healthy");
        assert_eq!(view.capabilities[0].availability_state, "available");
        assert_eq!(
            state
                .health_observation_visible(
                    &format!("provider:{}", registration.provider_id),
                    owner,
                )
                .unwrap()
                .unwrap()
                .health_state,
            HealthState::Healthy
        );
        drop(state);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn source_pin_and_extension_id_contracts_fail_closed() {
        let invalid_pin = CONTRACT.replace(
            "source_revision = \"c263c3997b4e6f2f7df5922e062a9e949e22f755\"",
            "source_revision = \"not-a-git-revision\"",
        );
        assert!(PortusBrowserContract::parse(&invalid_pin).is_err());
        assert!(
            contract()
                .chromium_native_messaging_spec("/home/master/.config", "not-an-extension-id",)
                .is_err()
        );
        assert!(
            contract()
                .chromium_native_messaging_spec(
                    "/home/master/.config",
                    "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz",
                )
                .is_err()
        );
    }

    #[test]
    fn repair_plan_is_bounded_and_contains_no_generic_shell_command() {
        let plan = contract().repair_plan();
        assert_eq!(plan.len(), 5);
        assert_eq!(plan[0], RepairStep::ProbeBroker);
        assert_eq!(plan[4], RepairStep::Reprobe);
    }
}
