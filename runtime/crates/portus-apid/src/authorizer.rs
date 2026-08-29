use portus_policy::{PolicyPaths, PolicySnapshot, PolicyTrust};
use portus_protected_api::{POLICY_ACTION, ProviderError, ProviderErrorCode, policy_resource};
use portus_protocol::{PolicyActionContext, PolicyEffect, Principal};

pub trait ProtectedApiAuthorizer: Send + Sync {
    fn effect(
        &self,
        principal: Principal,
        credential_ref: &str,
        operation: &str,
    ) -> Result<PolicyEffect, ProviderError>;
}

#[derive(Clone, Debug)]
pub struct FilesystemPolicyAuthorizer {
    paths: PolicyPaths,
    trust: PolicyTrust,
}

impl FilesystemPolicyAuthorizer {
    #[must_use]
    pub const fn new(paths: PolicyPaths, trust: PolicyTrust) -> Self {
        Self { paths, trust }
    }
}

impl ProtectedApiAuthorizer for FilesystemPolicyAuthorizer {
    fn effect(
        &self,
        principal: Principal,
        credential_ref: &str,
        operation: &str,
    ) -> Result<PolicyEffect, ProviderError> {
        let snapshot = PolicySnapshot::load(&self.paths, self.trust).map_err(|_| {
            ProviderError::new(
                ProviderErrorCode::PermissionDenied,
                "protected API policy is unavailable",
            )
        })?;
        let resource = policy_resource(credential_ref, operation)?;
        let decision = snapshot
            .evaluate(
                principal,
                &PolicyActionContext {
                    action: POLICY_ACTION.into(),
                    resource: Some(resource),
                },
            )
            .map_err(|_| {
                ProviderError::new(
                    ProviderErrorCode::PermissionDenied,
                    "protected API policy rejected the operation context",
                )
            })?;
        Ok(decision.effect)
    }
}
