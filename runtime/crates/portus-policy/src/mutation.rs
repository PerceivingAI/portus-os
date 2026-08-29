use crate::{
    BundleSelection, GrantDefinition, PolicyError, PolicyResult, PolicySnapshot, SubjectPolicy,
};
use portus_protocol::PolicyEffect;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AdminMutation {
    Grant {
        action: String,
        effect: PolicyEffect,
        resource: Option<String>,
        ack_root_equivalent: bool,
    },
    Revoke {
        action: String,
        resource: Option<String>,
    },
    BundleSet {
        bundle: String,
        enabled: bool,
    },
}

pub fn apply_admin_mutation(
    snapshot: &PolicySnapshot,
    subject: &mut SubjectPolicy,
    mutation: AdminMutation,
) -> PolicyResult<()> {
    match mutation {
        AdminMutation::Grant {
            action,
            effect,
            resource,
            ack_root_equivalent,
        } => {
            let definition = snapshot
                .action(&action)
                .ok_or_else(|| PolicyError::Invalid("unknown policy action".into()))?;
            if definition.root_equivalent && effect != PolicyEffect::Reject && !ack_root_equivalent
            {
                return Err(PolicyError::Invalid(
                    "root-equivalent grant requires explicit acknowledgement".into(),
                ));
            }
            if definition.resource_required != resource.is_some() {
                return Err(PolicyError::Invalid(
                    "grant resource shape does not match action".into(),
                ));
            }
            let resources = resource.clone().into_iter().collect::<Vec<_>>();
            subject
                .grants
                .retain(|grant| !same_scope(grant, &action, resource.as_deref()));
            subject.grants.push(GrantDefinition {
                action,
                effect,
                resources,
            });
        }
        AdminMutation::Revoke { action, resource } => {
            if snapshot.action(&action).is_none() {
                return Err(PolicyError::Invalid("unknown policy action".into()));
            }
            subject
                .grants
                .retain(|grant| !same_scope(grant, &action, resource.as_deref()));
        }
        AdminMutation::BundleSet { bundle, enabled } => {
            if snapshot.bundle(&bundle).is_none() {
                return Err(PolicyError::Invalid("unknown policy bundle".into()));
            }
            if let Some(selection) = subject
                .bundles
                .iter_mut()
                .find(|selection| selection.id == bundle)
            {
                selection.enabled = enabled;
            } else {
                subject.bundles.push(BundleSelection {
                    id: bundle,
                    enabled,
                });
            }
        }
    }
    snapshot.validate_subject(subject)
}

pub fn serialize_subject(subject: &SubjectPolicy) -> PolicyResult<String> {
    toml::to_string_pretty(subject)
        .map_err(|_| PolicyError::Invalid("failed to serialize subject policy".into()))
}

fn same_scope(grant: &GrantDefinition, action: &str, resource: Option<&str>) -> bool {
    if grant.action != action {
        return false;
    }
    match resource {
        Some(resource) => grant.resources.len() == 1 && grant.resources[0] == resource,
        None => grant.resources.is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActionDefinition, ActionRegistry, BundleDefinition, GlobalPolicy};
    use portus_protocol::PolicyEnforcementClass;

    fn snapshot() -> PolicySnapshot {
        PolicySnapshot::from_documents(
            GlobalPolicy {
                policy_version: 1,
                default_effect: PolicyEffect::Reject,
            },
            ActionRegistry {
                policy_version: 1,
                actions: vec![ActionDefinition {
                    id: "root.shell".into(),
                    label: "Root shell".into(),
                    class: PolicyEnforcementClass::RootEquivalent,
                    resource_kind: None,
                    resource_required: false,
                    root_equivalent: true,
                }],
            },
            vec![BundleDefinition {
                policy_version: 1,
                id: "system-administration".into(),
                label: "System Administration".into(),
                broad_default: true,
                grants: vec![],
            }],
            vec![],
        )
        .unwrap()
    }

    #[test]
    fn root_equivalent_grant_requires_acknowledgement() {
        let snapshot = snapshot();
        let mut subject = SubjectPolicy::empty(1000);
        assert!(
            apply_admin_mutation(
                &snapshot,
                &mut subject,
                AdminMutation::Grant {
                    action: "root.shell".into(),
                    effect: PolicyEffect::Allow,
                    resource: None,
                    ack_root_equivalent: false
                }
            )
            .is_err()
        );
        apply_admin_mutation(
            &snapshot,
            &mut subject,
            AdminMutation::Grant {
                action: "root.shell".into(),
                effect: PolicyEffect::Allow,
                resource: None,
                ack_root_equivalent: true,
            },
        )
        .unwrap();
        assert_eq!(subject.grants.len(), 1);
    }
}
