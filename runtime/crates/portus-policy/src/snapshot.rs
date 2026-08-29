use crate::model::{
    ActionDefinition, ActionRegistry, BundleDefinition, GlobalPolicy, MAX_BUNDLES, MAX_GRANTS,
    MAX_RESOURCES_PER_GRANT, MAX_SUBJECTS, POLICY_VERSION, PolicyError, PolicyPaths, PolicyResult,
    PolicyTrust, SubjectPolicy, action_map, validate_identifier, validate_resource,
};
use portus_protocol::{
    EffectiveBundleView, EffectiveGrantView, EffectivePolicyView, PolicyActionContext,
    PolicyDecision, PolicyEffect, Principal, SubjectPolicyView,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

const MAX_POLICY_FILE_BYTES: u64 = 256 * 1024;

#[derive(Clone, Debug)]
pub struct PolicySnapshot {
    global: GlobalPolicy,
    actions: BTreeMap<String, ActionDefinition>,
    bundles: BTreeMap<String, BundleDefinition>,
    subjects: BTreeMap<u32, SubjectPolicy>,
}

impl PolicySnapshot {
    pub fn load(paths: &PolicyPaths, trust: PolicyTrust) -> PolicyResult<Self> {
        validate_trust(paths, trust)?;
        let global: GlobalPolicy = parse_file(&paths.policy_path)?;
        let registry: ActionRegistry = parse_file(&paths.actions_path)?;
        let mut bundles = Vec::new();
        for path in toml_files(&paths.bundles_dir, MAX_BUNDLES)? {
            bundles.push(parse_file(&path)?);
        }
        let mut subjects = Vec::new();
        for path in toml_files(&paths.subjects_dir, MAX_SUBJECTS)? {
            let subject: SubjectPolicy = parse_file(&path)?;
            let expected_name = format!("{}.toml", subject.uid);
            if path.file_name().and_then(|value| value.to_str()) != Some(expected_name.as_str()) {
                return Err(PolicyError::Invalid(
                    "subject policy filename does not match embedded uid".into(),
                ));
            }
            subjects.push(subject);
        }
        Self::from_documents(global, registry, bundles, subjects)
    }

    pub fn from_documents(
        global: GlobalPolicy,
        registry: ActionRegistry,
        bundles: Vec<BundleDefinition>,
        subjects: Vec<SubjectPolicy>,
    ) -> PolicyResult<Self> {
        if global.policy_version != POLICY_VERSION || global.default_effect != PolicyEffect::Reject
        {
            return Err(PolicyError::Invalid(
                "policy v1 requires fail-closed default effect reject".into(),
            ));
        }
        let actions = action_map(&registry)?;
        if bundles.len() > MAX_BUNDLES {
            return Err(PolicyError::Invalid("too many policy bundles".into()));
        }
        let mut bundle_map = BTreeMap::new();
        for bundle in bundles {
            validate_bundle(&bundle, &actions)?;
            if bundle_map.insert(bundle.id.clone(), bundle).is_some() {
                return Err(PolicyError::Invalid("duplicate bundle id".into()));
            }
        }
        if subjects.len() > MAX_SUBJECTS {
            return Err(PolicyError::Invalid("too many policy subjects".into()));
        }
        let mut subject_map = BTreeMap::new();
        for subject in subjects {
            validate_subject(&subject, &actions, &bundle_map)?;
            if subject_map.insert(subject.uid, subject).is_some() {
                return Err(PolicyError::Invalid("duplicate subject uid".into()));
            }
        }
        Ok(Self {
            global,
            actions,
            bundles: bundle_map,
            subjects: subject_map,
        })
    }

    #[must_use]
    pub fn subject(&self, uid: u32) -> SubjectPolicy {
        self.subjects
            .get(&uid)
            .cloned()
            .unwrap_or_else(|| SubjectPolicy::empty(uid))
    }

    pub fn effective(&self, principal: Principal) -> PolicyResult<EffectivePolicyView> {
        let subject = self.subject(principal.uid());
        let grants = self.resolved_grants(&subject)?;
        let has_root_equivalent_authority = grants
            .iter()
            .any(|grant| grant.root_equivalent && grant.effect != PolicyEffect::Reject);
        let bundles = subject
            .bundles
            .iter()
            .map(|bundle| EffectiveBundleView {
                id: bundle.id.clone(),
                enabled: bundle.enabled,
            })
            .collect();
        Ok(EffectivePolicyView {
            principal,
            policy_version: POLICY_VERSION,
            bundles,
            grants,
            has_root_equivalent_authority,
        })
    }

    pub fn subject_view(&self, uid: u32) -> PolicyResult<SubjectPolicyView> {
        let subject = self.subject(uid);
        let bundles = subject
            .bundles
            .iter()
            .map(|bundle| EffectiveBundleView {
                id: bundle.id.clone(),
                enabled: bundle.enabled,
            })
            .collect();
        Ok(SubjectPolicyView {
            uid,
            label: subject.label.clone(),
            bundles,
            grants: self.resolved_grants(&subject)?,
        })
    }

    pub fn evaluate(
        &self,
        principal: Principal,
        context: &PolicyActionContext,
    ) -> PolicyResult<PolicyDecision> {
        let action = self
            .actions
            .get(&context.action)
            .ok_or_else(|| PolicyError::Invalid("unknown policy action".into()))?;
        validate_context(action, context)?;
        let subject = self.subject(principal.uid());
        let key = (context.action.clone(), context.resource.clone());
        let resolved = self.resolved_grant_map(&subject)?;
        let (effect, reason_code) = resolved.get(&key).map_or(
            (self.global.default_effect, "default_reject".to_string()),
            |grant| (grant.effect, grant.source.clone()),
        );
        Ok(PolicyDecision {
            principal,
            action: context.action.clone(),
            resource: context.resource.clone(),
            effect,
            reason_code,
            enforcement_class: action.class,
            root_equivalent: action.root_equivalent,
        })
    }

    pub fn validate_subject(&self, subject: &SubjectPolicy) -> PolicyResult<()> {
        validate_subject(subject, &self.actions, &self.bundles)
    }

    pub fn replace_subject(&mut self, subject: SubjectPolicy) -> PolicyResult<()> {
        self.validate_subject(&subject)?;
        self.subjects.insert(subject.uid, subject);
        Ok(())
    }

    #[must_use]
    pub fn action(&self, action: &str) -> Option<&ActionDefinition> {
        self.actions.get(action)
    }

    #[must_use]
    pub fn bundle(&self, bundle: &str) -> Option<&BundleDefinition> {
        self.bundles.get(bundle)
    }

    fn resolved_grants(&self, subject: &SubjectPolicy) -> PolicyResult<Vec<EffectiveGrantView>> {
        Ok(self.resolved_grant_map(subject)?.into_values().collect())
    }

    fn resolved_grant_map(
        &self,
        subject: &SubjectPolicy,
    ) -> PolicyResult<BTreeMap<(String, Option<String>), EffectiveGrantView>> {
        let mut map: BTreeMap<(String, Option<String>), EffectiveGrantView> = BTreeMap::new();
        for selection in subject.bundles.iter().filter(|selection| selection.enabled) {
            let bundle = self
                .bundles
                .get(&selection.id)
                .ok_or_else(|| PolicyError::Invalid("subject references unknown bundle".into()))?;
            for grant in &bundle.grants {
                for key in grant_keys(grant, &self.actions)? {
                    let action = &self.actions[&grant.action];
                    if let Some(existing) = map.get(&key) {
                        if existing.effect != grant.effect {
                            return Err(PolicyError::Invalid(
                                "enabled bundles contain conflicting grants".into(),
                            ));
                        }
                        continue;
                    }
                    map.insert(
                        key.clone(),
                        EffectiveGrantView {
                            action: key.0,
                            resource: key.1,
                            effect: grant.effect,
                            source: format!("bundle:{}", bundle.id),
                            root_equivalent: action.root_equivalent,
                        },
                    );
                }
            }
        }
        for grant in &subject.grants {
            for key in grant_keys(grant, &self.actions)? {
                let action = &self.actions[&grant.action];
                map.insert(
                    key.clone(),
                    EffectiveGrantView {
                        action: key.0,
                        resource: key.1,
                        effect: grant.effect,
                        source: "explicit_grant".into(),
                        root_equivalent: action.root_equivalent,
                    },
                );
            }
        }
        Ok(map)
    }
}

fn validate_bundle(
    bundle: &BundleDefinition,
    actions: &BTreeMap<String, ActionDefinition>,
) -> PolicyResult<()> {
    if bundle.policy_version != POLICY_VERSION {
        return Err(PolicyError::Invalid("bundle version is invalid".into()));
    }
    validate_identifier(&bundle.id, "bundle id")?;
    if bundle.label.is_empty() || bundle.label.len() > 128 || bundle.grants.len() > MAX_GRANTS {
        return Err(PolicyError::Invalid(
            "bundle label/grant count is invalid".into(),
        ));
    }
    validate_grants(&bundle.grants, actions, true)
}

fn validate_subject(
    subject: &SubjectPolicy,
    actions: &BTreeMap<String, ActionDefinition>,
    bundles: &BTreeMap<String, BundleDefinition>,
) -> PolicyResult<()> {
    if subject.policy_version != POLICY_VERSION
        || subject.grants.len() > MAX_GRANTS
        || subject.bundles.len() > MAX_BUNDLES
    {
        return Err(PolicyError::Invalid(
            "subject version/count is invalid".into(),
        ));
    }
    if subject
        .label
        .as_ref()
        .is_some_and(|label| label.is_empty() || label.len() > 128)
    {
        return Err(PolicyError::Invalid("subject label is invalid".into()));
    }
    let mut seen = BTreeSet::new();
    for selection in &subject.bundles {
        if !bundles.contains_key(&selection.id) || !seen.insert(selection.id.clone()) {
            return Err(PolicyError::Invalid(
                "subject bundle selection is invalid".into(),
            ));
        }
    }
    validate_grants(&subject.grants, actions, false)
}

fn validate_grants(
    grants: &[crate::GrantDefinition],
    actions: &BTreeMap<String, ActionDefinition>,
    from_bundle: bool,
) -> PolicyResult<()> {
    let mut seen = BTreeSet::new();
    for grant in grants {
        let action = actions
            .get(&grant.action)
            .ok_or_else(|| PolicyError::Invalid("grant references unknown action".into()))?;
        if from_bundle && action.root_equivalent {
            return Err(PolicyError::Invalid(
                "bundle cannot contain root-equivalent grant".into(),
            ));
        }
        let keys = grant_keys(grant, actions)?;
        for key in keys {
            if !seen.insert(key) {
                return Err(PolicyError::Invalid("duplicate grant scope".into()));
            }
        }
    }
    Ok(())
}

fn grant_keys(
    grant: &crate::GrantDefinition,
    actions: &BTreeMap<String, ActionDefinition>,
) -> PolicyResult<Vec<(String, Option<String>)>> {
    let action = actions
        .get(&grant.action)
        .ok_or_else(|| PolicyError::Invalid("grant references unknown action".into()))?;
    if action.resource_required {
        if grant.resources.is_empty() || grant.resources.len() > MAX_RESOURCES_PER_GRANT {
            return Err(PolicyError::Invalid(
                "resource-scoped grant has invalid resource count".into(),
            ));
        }
        let mut seen = BTreeSet::new();
        let mut keys = Vec::new();
        for resource in &grant.resources {
            validate_resource(resource)?;
            if !seen.insert(resource.clone()) {
                return Err(PolicyError::Invalid("duplicate grant resource".into()));
            }
            keys.push((grant.action.clone(), Some(resource.clone())));
        }
        Ok(keys)
    } else {
        if !grant.resources.is_empty() {
            return Err(PolicyError::Invalid(
                "resource-free action cannot carry resources".into(),
            ));
        }
        Ok(vec![(grant.action.clone(), None)])
    }
}

fn validate_context(action: &ActionDefinition, context: &PolicyActionContext) -> PolicyResult<()> {
    validate_identifier(&context.action, "action id")?;
    match (&context.resource, action.resource_required) {
        (Some(resource), true) => validate_resource(resource),
        (None, true) => Err(PolicyError::Invalid("action requires a resource".into())),
        (Some(_), false) => Err(PolicyError::Invalid(
            "action does not accept a resource".into(),
        )),
        (None, false) => Ok(()),
    }
}

fn parse_file<T: serde::de::DeserializeOwned>(path: &Path) -> PolicyResult<T> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() || metadata.len() > MAX_POLICY_FILE_BYTES {
        return Err(PolicyError::Invalid(
            "policy file is missing, non-regular, or oversized".into(),
        ));
    }
    let text = fs::read_to_string(path)?;
    toml::from_str(&text)
        .map_err(|_| PolicyError::Parse(format!("{} does not match policy schema", path.display())))
}

fn toml_files(directory: &Path, maximum: usize) -> PolicyResult<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.extension().and_then(|value| value.to_str()) == Some("toml") {
            files.push(path);
            if files.len() > maximum {
                return Err(PolicyError::Invalid(
                    "policy directory contains too many TOML documents".into(),
                ));
            }
        }
    }
    files.sort();
    Ok(files)
}

fn validate_trust(paths: &PolicyPaths, trust: PolicyTrust) -> PolicyResult<()> {
    if trust == PolicyTrust::PretrustedFixture {
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let policy_parent = paths
            .policy_path
            .parent()
            .ok_or_else(|| PolicyError::Permission("policy path has no parent directory".into()))?;
        let actions_parent = paths.actions_path.parent().ok_or_else(|| {
            PolicyError::Permission("action registry path has no parent directory".into())
        })?;
        for directory in [
            policy_parent,
            actions_parent,
            &paths.subjects_dir,
            &paths.bundles_dir,
        ] {
            let metadata = fs::symlink_metadata(directory)?;
            if metadata.file_type().is_symlink()
                || !metadata.is_dir()
                || metadata.uid() != 0
                || metadata.permissions().mode() & 0o022 != 0
            {
                return Err(PolicyError::Permission(format!(
                    "{} is not a trusted root-owned policy directory",
                    directory.display()
                )));
            }
        }
        for file in [&paths.policy_path, &paths.actions_path] {
            let metadata = fs::symlink_metadata(file)?;
            if metadata.file_type().is_symlink()
                || !metadata.is_file()
                || metadata.uid() != 0
                || metadata.permissions().mode() & 0o022 != 0
            {
                return Err(PolicyError::Permission(format!(
                    "{} is not trusted root-owned policy material",
                    file.display()
                )));
            }
        }
        for directory in [&paths.subjects_dir, &paths.bundles_dir] {
            let maximum = if *directory == paths.subjects_dir.as_path() {
                MAX_SUBJECTS
            } else {
                MAX_BUNDLES
            };
            for path in toml_files(directory, maximum)? {
                let metadata = fs::symlink_metadata(&path)?;
                if metadata.file_type().is_symlink()
                    || !metadata.is_file()
                    || metadata.uid() != 0
                    || metadata.permissions().mode() & 0o022 != 0
                {
                    return Err(PolicyError::Permission(format!(
                        "{} is not trusted root-owned policy material",
                        path.display()
                    )));
                }
            }
        }
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = paths;
        Err(PolicyError::UnsupportedPlatform)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BundleSelection, GrantDefinition};
    use portus_protocol::PolicyEnforcementClass;

    fn snapshot() -> PolicySnapshot {
        PolicySnapshot::from_documents(
            GlobalPolicy {
                policy_version: 1,
                default_effect: PolicyEffect::Reject,
            },
            ActionRegistry {
                policy_version: 1,
                actions: vec![
                    ActionDefinition {
                        id: "service.restart".into(),
                        label: "Restart service".into(),
                        class: PolicyEnforcementClass::PrivilegedTypedOperation,
                        resource_kind: Some("openrc_service".into()),
                        resource_required: true,
                        root_equivalent: false,
                    },
                    ActionDefinition {
                        id: "root.shell".into(),
                        label: "Root shell".into(),
                        class: PolicyEnforcementClass::RootEquivalent,
                        resource_kind: None,
                        resource_required: false,
                        root_equivalent: true,
                    },
                ],
            },
            vec![BundleDefinition {
                policy_version: 1,
                id: "system-administration".into(),
                label: "System Administration".into(),
                broad_default: true,
                grants: vec![GrantDefinition {
                    action: "service.restart".into(),
                    effect: PolicyEffect::Allow,
                    resources: vec!["portusd".into()],
                }],
            }],
            vec![SubjectPolicy {
                policy_version: 1,
                uid: 1000,
                label: Some("master".into()),
                bundles: vec![BundleSelection {
                    id: "system-administration".into(),
                    enabled: true,
                }],
                grants: vec![GrantDefinition {
                    action: "service.restart".into(),
                    effect: PolicyEffect::Reject,
                    resources: vec!["sshd".into()],
                }],
            }],
        )
        .unwrap()
    }

    #[test]
    fn subject_filename_must_match_embedded_uid() {
        let dir = std::env::temp_dir().join(format!(
            "portus-policy-filename-{}",
            portus_protocol::TaskId::new()
        ));
        let subjects = dir.join("subjects.d");
        let bundles = dir.join("bundles");
        fs::create_dir_all(&subjects).unwrap();
        fs::create_dir_all(&bundles).unwrap();
        fs::write(
            dir.join("policy.toml"),
            "policy_version = 1\ndefault_effect = \"reject\"\n",
        )
        .unwrap();
        fs::write(
            dir.join("actions.toml"),
            "policy_version = 1\nactions = []\n",
        )
        .unwrap();
        fs::write(
            subjects.join("1000.toml"),
            "policy_version = 1\nuid = 2000\n",
        )
        .unwrap();
        let paths = PolicyPaths {
            policy_path: dir.join("policy.toml"),
            subjects_dir: subjects,
            actions_path: dir.join("actions.toml"),
            bundles_dir: bundles,
        };
        let result = PolicySnapshot::load(&paths, PolicyTrust::PretrustedFixture);
        assert!(matches!(result, Err(PolicyError::Invalid(_))));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn policy_subject_count_is_bounded() {
        let mut subjects = Vec::new();
        for uid in 0..=MAX_SUBJECTS {
            subjects.push(SubjectPolicy::empty(u32::try_from(uid).unwrap()));
        }
        let result = PolicySnapshot::from_documents(
            GlobalPolicy {
                policy_version: 1,
                default_effect: PolicyEffect::Reject,
            },
            ActionRegistry {
                policy_version: 1,
                actions: vec![],
            },
            vec![],
            subjects,
        );
        assert!(matches!(result, Err(PolicyError::Invalid(_))));
    }

    #[test]
    fn explicit_grant_overrides_bundle_and_default_is_reject() {
        let policy = snapshot();
        let principal = Principal::new(1000, 1000);
        assert_eq!(
            policy
                .evaluate(
                    principal,
                    &PolicyActionContext {
                        action: "service.restart".into(),
                        resource: Some("portusd".into())
                    }
                )
                .unwrap()
                .effect,
            PolicyEffect::Allow
        );
        assert_eq!(
            policy
                .evaluate(
                    principal,
                    &PolicyActionContext {
                        action: "service.restart".into(),
                        resource: Some("sshd".into())
                    }
                )
                .unwrap()
                .effect,
            PolicyEffect::Reject
        );
        assert_eq!(
            policy
                .evaluate(
                    principal,
                    &PolicyActionContext {
                        action: "root.shell".into(),
                        resource: None
                    }
                )
                .unwrap()
                .effect,
            PolicyEffect::Reject
        );
    }

    #[test]
    fn bundles_cannot_hide_root_equivalent_authority() {
        let error = PolicySnapshot::from_documents(
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
                grants: vec![GrantDefinition {
                    action: "root.shell".into(),
                    effect: PolicyEffect::Allow,
                    resources: vec![],
                }],
            }],
            vec![],
        )
        .unwrap_err();
        assert!(error.to_string().contains("root-equivalent"));
    }
}
