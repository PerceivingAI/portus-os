use portus_protocol::{IndexHealthState, Principal};
use portus_state::PortusState;
use serde::Serialize;
use std::{
    env,
    fs::OpenOptions,
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug)]
pub struct DoctorContext {
    pub socket_path: PathBuf,
    pub state_path: PathBuf,
    pub capabilities_dir: PathBuf,
}

impl Default for DoctorContext {
    fn default() -> Self {
        Self {
            socket_path: PathBuf::from("/run/portus/portusd.sock"),
            state_path: PathBuf::from(portus_state::CANONICAL_DATABASE_PATH),
            capabilities_dir: PathBuf::from("/etc/portus/capabilities"),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct DoctorCheck {
    pub group: &'static str,
    pub check: &'static str,
    pub status: &'static str,
    pub reason: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct DoctorReport {
    pub schema_version: u32,
    pub generated_at_ms: i64,
    pub checks: Vec<DoctorCheck>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle_path: Option<String>,
}

impl DoctorReport {
    #[must_use]
    pub fn human_lines(&self) -> Vec<String> {
        self.checks
            .iter()
            .map(|check| {
                format!(
                    "{:<10} {:<18} {:<12} {}",
                    check.group, check.check, check.status, check.reason
                )
            })
            .collect()
    }
}

impl DoctorContext {
    #[must_use]
    pub fn run(&self, domain: Option<crate::DoctorDomain>) -> DoctorReport {
        let mut checks = Vec::new();
        if domain.is_none() || domain == Some(crate::DoctorDomain::Runtime) {
            checks.push(self.runtime_check());
        }
        if domain.is_none() || domain == Some(crate::DoctorDomain::State) {
            checks.push(self.state_check());
        }
        if domain.is_none() || domain == Some(crate::DoctorDomain::Index) {
            checks.push(self.index_check());
        }
        if domain.is_none() || domain == Some(crate::DoctorDomain::Providers) {
            checks.push(self.providers_check());
        }
        if domain.is_none() || domain == Some(crate::DoctorDomain::Codex) {
            checks.push(codex_check());
        }
        DoctorReport {
            schema_version: 1,
            generated_at_ms: unix_time_ms(),
            checks,
            bundle_path: None,
        }
    }

    pub fn write_bundle(&self, report: &DoctorReport, path: &Path) -> io::Result<()> {
        let encoded = serde_json::to_vec_pretty(report)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        if encoded.len() > 64 * 1024 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "diagnostic bundle exceeds the 64 KiB first-ISO bound",
            ));
        }
        let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
        file.write_all(&encoded)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        Ok(())
    }

    fn runtime_check(&self) -> DoctorCheck {
        if !self.socket_path.exists() {
            return DoctorCheck {
                group: "runtime",
                check: "portusd socket",
                status: "unavailable",
                reason: "canonical portusd socket is missing",
            };
        }
        #[cfg(target_os = "linux")]
        {
            match std::os::unix::net::UnixStream::connect(&self.socket_path) {
                Ok(_) => DoctorCheck {
                    group: "runtime",
                    check: "portusd socket",
                    status: "reachable",
                    reason: "Unix socket accepts a local connection",
                },
                Err(_) => DoctorCheck {
                    group: "runtime",
                    check: "portusd socket",
                    status: "unavailable",
                    reason: "Unix socket exists but cannot be connected",
                },
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            DoctorCheck {
                group: "runtime",
                check: "portusd socket",
                status: "unknown",
                reason: "runtime socket connectivity requires Linux",
            }
        }
    }

    fn state_check(&self) -> DoctorCheck {
        if !self.state_path.exists() {
            return DoctorCheck {
                group: "state",
                check: "portus.db",
                status: "unavailable",
                reason: "canonical Portus state database is missing",
            };
        }
        match PortusState::open_read_only(&self.state_path) {
            Ok(state) => match state.integrity_check() {
                Ok(()) => DoctorCheck {
                    group: "state",
                    check: "portus.db",
                    status: "healthy",
                    reason: "database opened read-only and passed integrity check",
                },
                Err(_) => DoctorCheck {
                    group: "state",
                    check: "portus.db",
                    status: "degraded",
                    reason: "database integrity check failed",
                },
            },
            Err(_) => DoctorCheck {
                group: "state",
                check: "portus.db",
                status: "unavailable",
                reason: "database could not be opened safely read-only",
            },
        }
    }

    fn index_check(&self) -> DoctorCheck {
        if !self.state_path.exists() {
            return DoctorCheck {
                group: "index",
                check: "derived state",
                status: "unavailable",
                reason: "Portus state database is missing, so index metadata cannot be inspected",
            };
        }
        let state = match PortusState::open_read_only(&self.state_path) {
            Ok(state) => state,
            Err(_) => {
                return DoctorCheck {
                    group: "index",
                    check: "derived state",
                    status: "unavailable",
                    reason: "Portus state database could not be opened safely read-only",
                };
            }
        };
        let status = match state.index_runtime_status(Principal::new(0, 0)) {
            Ok(status) => status,
            Err(_) => {
                return DoctorCheck {
                    group: "index",
                    check: "derived state",
                    status: "unavailable",
                    reason: "System Index metadata is missing or unreadable",
                };
            }
        };
        match status.state {
            IndexHealthState::Healthy => DoctorCheck {
                group: "index",
                check: "derived state",
                status: "healthy",
                reason: "machine-scoped System Index metadata is readable and healthy",
            },
            IndexHealthState::Degraded => DoctorCheck {
                group: "index",
                check: "derived state",
                status: "degraded",
                reason: "System Index metadata reports one or more degraded machine sources",
            },
            IndexHealthState::Unavailable => DoctorCheck {
                group: "index",
                check: "derived state",
                status: "unavailable",
                reason: "System Index metadata reports the index as unavailable",
            },
            IndexHealthState::Initializing | IndexHealthState::Rebuilding => DoctorCheck {
                group: "index",
                check: "derived state",
                status: "unknown",
                reason: "System Index metadata is readable but not in a settled health state",
            },
        }
    }

    fn providers_check(&self) -> DoctorCheck {
        if !self.capabilities_dir.is_dir() {
            return DoctorCheck {
                group: "providers",
                check: "registry",
                status: "unknown",
                reason: "capability manifest directory is not installed yet",
            };
        }
        if !self.state_path.exists() {
            return DoctorCheck {
                group: "providers",
                check: "registry",
                status: "unknown",
                reason: "provider manifests exist but Portus state is unavailable for registry inspection",
            };
        }
        match PortusState::open_read_only(&self.state_path)
            .and_then(|state| state.active_system_provider_count())
        {
            Ok(_) => DoctorCheck {
                group: "providers",
                check: "registry",
                status: "healthy",
                reason: "provider manifest directory and durable registry are readable",
            },
            Err(_) => DoctorCheck {
                group: "providers",
                check: "registry",
                status: "degraded",
                reason: "provider manifest directory exists but durable registry cannot be inspected safely",
            },
        }
    }
}

fn unix_time_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

fn codex_check() -> DoctorCheck {
    if find_on_path("codex") {
        DoctorCheck {
            group: "codex",
            check: "binary",
            status: "present",
            reason: "codex executable is present and executable on PATH",
        }
    } else {
        DoctorCheck {
            group: "codex",
            check: "binary",
            status: "unavailable",
            reason: "codex executable was not found on PATH",
        }
    }
}

fn find_on_path(program: &str) -> bool {
    env::var_os("PATH").is_some_and(|paths| {
        env::split_paths(&paths).any(|directory| executable_candidate_exists(&directory, program))
    })
}

fn executable_candidate_exists(directory: &Path, program: &str) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let candidate = directory.join(program);
        candidate
            .metadata()
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
    }
    #[cfg(windows)]
    {
        directory.join(format!("{program}.exe")).is_file() || directory.join(program).is_file()
    }
    #[cfg(not(any(unix, windows)))]
    {
        directory.join(program).is_file()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn missing_runtime_is_diagnosed_without_daemon_business_logic() {
        let dir =
            std::env::temp_dir().join(format!("portus-doctor-{}", portus_protocol::TaskId::new()));
        fs::create_dir_all(&dir).unwrap();
        let context = DoctorContext {
            socket_path: dir.join("missing.sock"),
            state_path: dir.join("missing.db"),
            capabilities_dir: dir.join("capabilities"),
        };
        let report = context.run(Some(crate::DoctorDomain::Runtime));
        assert_eq!(report.checks[0].status, "unavailable");
        assert!(report.checks[0].reason.contains("missing"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn index_doctor_reads_p6_metadata_without_portusd() {
        let dir = std::env::temp_dir().join(format!(
            "portus-doctor-index-{}",
            portus_protocol::TaskId::new()
        ));
        fs::create_dir_all(&dir).unwrap();
        let state_path = dir.join("portus.db");
        let state = PortusState::open(&state_path).unwrap();
        state
            .set_index_runtime_state(IndexHealthState::Healthy, "ready", 10, true)
            .unwrap();
        drop(state);
        let context = DoctorContext {
            socket_path: dir.join("missing.sock"),
            state_path,
            capabilities_dir: dir.join("capabilities"),
        };
        let report = context.run(Some(crate::DoctorDomain::Index));
        assert_eq!(report.checks[0].status, "healthy");
        assert!(report.checks[0].reason.contains("readable"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn diagnostic_bundle_is_allowlisted_bounded_and_refuses_overwrite() {
        let dir = std::env::temp_dir().join(format!(
            "portus-doctor-bundle-{}",
            portus_protocol::TaskId::new()
        ));
        fs::create_dir_all(&dir).unwrap();
        let context = DoctorContext {
            socket_path: dir.join("missing.sock"),
            state_path: dir.join("missing.db"),
            capabilities_dir: dir.join("capabilities"),
        };
        let report = context.run(None);
        let path = dir.join("doctor.json");
        context.write_bundle(&report, &path).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(value["schema_version"], 1);
        assert!(value["checks"].is_array());
        let encoded = value.to_string();
        for forbidden in ["environment", "credential", "authorization", "payload"] {
            assert!(!encoded.to_ascii_lowercase().contains(forbidden));
        }
        assert!(context.write_bundle(&report, &path).is_err());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn state_doctor_uses_read_only_integrity_path() {
        let dir = std::env::temp_dir().join(format!(
            "portus-doctor-state-{}",
            portus_protocol::TaskId::new()
        ));
        fs::create_dir_all(&dir).unwrap();
        let state_path = dir.join("portus.db");
        drop(PortusState::open(&state_path).unwrap());
        let context = DoctorContext {
            socket_path: dir.join("missing.sock"),
            state_path,
            capabilities_dir: dir.join("capabilities"),
        };
        let report = context.run(Some(crate::DoctorDomain::State));
        assert_eq!(report.checks[0].status, "healthy");
        let _ = fs::remove_dir_all(dir);
    }
}
