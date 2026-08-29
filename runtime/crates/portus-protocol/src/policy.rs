use crate::Principal;
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

macro_rules! policy_string_enum {
    ($name:ident { $($variant:ident => $value:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($variant),+ }
        impl $name {
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $value),+ }
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(self.as_str()) }
        }
        impl FromStr for $name {
            type Err = String;
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                match value { $($value => Ok(Self::$variant),)+ _ => Err(format!("unknown {} value", stringify!($name))) }
            }
        }
    };
}

policy_string_enum!(PolicyEffect {
    Allow => "allow",
    Prompt => "prompt",
    Reject => "reject",
});

policy_string_enum!(PolicyEnforcementClass {
    UserNative => "user_native",
    ResourceGrant => "resource_grant",
    PortusProviderPolicy => "portus_provider_policy",
    PrivilegedTypedOperation => "privileged_typed_operation",
    RootEquivalent => "root_equivalent",
});

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyActionContext {
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyDecision {
    pub principal: Principal,
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,
    pub effect: PolicyEffect,
    pub reason_code: String,
    pub enforcement_class: PolicyEnforcementClass,
    pub root_equivalent: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectiveGrantView {
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,
    pub effect: PolicyEffect,
    pub source: String,
    pub root_equivalent: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectiveBundleView {
    pub id: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectivePolicyView {
    pub principal: Principal,
    pub policy_version: u32,
    pub bundles: Vec<EffectiveBundleView>,
    pub grants: Vec<EffectiveGrantView>,
    pub has_root_equivalent_authority: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubjectPolicyView {
    pub uid: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub bundles: Vec<EffectiveBundleView>,
    pub grants: Vec<EffectiveGrantView>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrivilegedOperationRequest {
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrivilegedOperationResult {
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,
    pub safe_summary: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_enums_have_stable_wire_values() {
        assert_eq!(PolicyEffect::Allow.as_str(), "allow");
        assert_eq!(PolicyEffect::Prompt.as_str(), "prompt");
        assert_eq!(PolicyEffect::Reject.as_str(), "reject");
        assert_eq!(
            PolicyEnforcementClass::PrivilegedTypedOperation.as_str(),
            "privileged_typed_operation"
        );
    }

    #[test]
    fn policy_context_denies_unknown_fields() {
        let value =
            serde_json::json!({"action":"service.restart","resource":"demo","caller_uid":0});
        assert!(serde_json::from_value::<PolicyActionContext>(value).is_err());
    }
}
