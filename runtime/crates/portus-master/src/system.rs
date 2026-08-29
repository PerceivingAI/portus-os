use crate::{MasterLaunchError, MasterLaunchResult};
use std::path::PathBuf;

#[cfg(not(target_os = "linux"))]
use std::env;
#[cfg(target_os = "linux")]
use std::fs;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaunchIdentity {
    pub username: String,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
    pub home: PathBuf,
    pub shell: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaunchLayout {
    pub workspace_root: PathBuf,
    pub tmux_program: PathBuf,
    pub master_program: PathBuf,
    pub codex_program: PathBuf,
    pub portus_os_program: PathBuf,
}

impl Default for LaunchLayout {
    fn default() -> Self {
        Self {
            workspace_root: PathBuf::from("/workspace"),
            tmux_program: PathBuf::from("tmux"),
            master_program: PathBuf::from("portus-master"),
            codex_program: PathBuf::from("codex"),
            portus_os_program: PathBuf::from("portus-os"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaunchContext {
    pub identity: LaunchIdentity,
    pub layout: LaunchLayout,
}

impl LaunchContext {
    pub fn system() -> MasterLaunchResult<Self> {
        Ok(Self {
            identity: system_identity()?,
            layout: LaunchLayout::default(),
        })
    }

    pub fn validate(&self) -> MasterLaunchResult<()> {
        if self.identity.uid == Some(0) {
            return Err(MasterLaunchError::RootExecution);
        }
        validate_username(&self.identity.username)?;
        if self.identity.home.as_os_str().is_empty() {
            return Err(MasterLaunchError::InvalidIdentity(
                "home directory is empty".into(),
            ));
        }
        if self.identity.shell.as_os_str().is_empty() {
            return Err(MasterLaunchError::InvalidIdentity(
                "interactive shell is empty".into(),
            ));
        }
        Ok(())
    }

    #[must_use]
    pub fn user_workspace(&self) -> PathBuf {
        self.layout.workspace_root.join(&self.identity.username)
    }

    #[must_use]
    pub fn master_workspace(&self) -> PathBuf {
        self.user_workspace().join("master")
    }
}

fn validate_username(username: &str) -> MasterLaunchResult<()> {
    if username.is_empty() || username == "." || username == ".." || username.len() > 128 {
        return Err(MasterLaunchError::InvalidIdentity(
            "username is empty, reserved, or too long".into(),
        ));
    }
    if !username
        .chars()
        .all(|value| value.is_ascii_alphanumeric() || matches!(value, '_' | '-' | '.'))
    {
        return Err(MasterLaunchError::InvalidIdentity(
            "username contains unsafe path characters".into(),
        ));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn system_identity() -> MasterLaunchResult<LaunchIdentity> {
    let status = fs::read_to_string("/proc/self/status").map_err(|error| {
        MasterLaunchError::IdentityDiscovery(format!("cannot read /proc/self/status: {error}"))
    })?;
    let uid = parse_status_effective_id(&status, "Uid:")?;
    let gid = parse_status_effective_id(&status, "Gid:")?;
    let passwd = fs::read_to_string("/etc/passwd").map_err(|error| {
        MasterLaunchError::IdentityDiscovery(format!("cannot read /etc/passwd: {error}"))
    })?;
    let (username, home, shell) = parse_passwd_identity(&passwd, uid)?;
    Ok(LaunchIdentity {
        username,
        uid: Some(uid),
        gid: Some(gid),
        home,
        shell,
    })
}

#[cfg(any(target_os = "linux", test))]
fn parse_status_effective_id(status: &str, key: &str) -> MasterLaunchResult<u32> {
    let line = status
        .lines()
        .find(|line| line.starts_with(key))
        .ok_or_else(|| MasterLaunchError::IdentityDiscovery(format!("missing {key} field")))?;
    let value = line.split_whitespace().nth(2).ok_or_else(|| {
        MasterLaunchError::IdentityDiscovery(format!("missing effective {key} value"))
    })?;
    value
        .parse::<u32>()
        .map_err(|_| MasterLaunchError::IdentityDiscovery(format!("invalid numeric {key} value")))
}

#[cfg(any(target_os = "linux", test))]
fn parse_passwd_identity(passwd: &str, uid: u32) -> MasterLaunchResult<(String, PathBuf, PathBuf)> {
    for line in passwd.lines() {
        let fields = line.split(':').collect::<Vec<_>>();
        if fields.len() < 7 || fields[2].parse::<u32>().ok() != Some(uid) {
            continue;
        }
        let username = fields[0].to_string();
        validate_username(&username)?;
        let home = PathBuf::from(fields[5]);
        let shell = if fields[6].is_empty() {
            PathBuf::from("/bin/sh")
        } else {
            PathBuf::from(fields[6])
        };
        return Ok((username, home, shell));
    }
    Err(MasterLaunchError::IdentityDiscovery(format!(
        "UID {uid} has no local /etc/passwd entry"
    )))
}

#[cfg(not(target_os = "linux"))]
fn system_identity() -> MasterLaunchResult<LaunchIdentity> {
    let username = env::var("USERNAME")
        .or_else(|_| env::var("USER"))
        .map_err(|_| {
            MasterLaunchError::IdentityDiscovery("username environment is absent".into())
        })?;
    validate_username(&username)?;
    let home = env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from)
        .ok_or_else(|| MasterLaunchError::IdentityDiscovery("home environment is absent".into()))?;
    let shell = env::var_os("COMSPEC")
        .or_else(|| env::var_os("SHELL"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("cmd.exe"));
    Ok(LaunchIdentity {
        username,
        uid: None,
        gid: None,
        home,
        shell,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(username: &str, uid: Option<u32>) -> LaunchContext {
        LaunchContext {
            identity: LaunchIdentity {
                username: username.into(),
                uid,
                gid: Some(1000),
                home: PathBuf::from("/home/demo"),
                shell: PathBuf::from("/bin/sh"),
            },
            layout: LaunchLayout::default(),
        }
    }

    #[test]
    fn root_and_unsafe_usernames_fail_closed() {
        assert!(matches!(
            context("demo", Some(0)).validate(),
            Err(MasterLaunchError::RootExecution)
        ));
        assert!(matches!(
            context("../root", Some(1000)).validate(),
            Err(MasterLaunchError::InvalidIdentity(_))
        ));
    }

    #[test]
    fn linux_status_parser_uses_effective_not_real_identity() {
        let status = "Name:\tportus\nUid:\t1000\t2000\t3000\t4000\nGid:\t100\t200\t300\t400\n";
        assert_eq!(parse_status_effective_id(status, "Uid:").unwrap(), 2000);
        assert_eq!(parse_status_effective_id(status, "Gid:").unwrap(), 200);
    }

    #[test]
    fn passwd_parser_resolves_the_effective_uid_record() {
        let passwd =
            "root:x:0:0:root:/root:/bin/sh\nmaster:x:2000:2000:Master:/home/master:/bin/bash\n";
        let (username, home, shell) = parse_passwd_identity(passwd, 2000).unwrap();
        assert_eq!(username, "master");
        assert_eq!(home, PathBuf::from("/home/master"));
        assert_eq!(shell, PathBuf::from("/bin/bash"));
    }

    #[test]
    fn master_workspace_is_deterministic() {
        assert_eq!(
            context("demo", Some(1000)).master_workspace(),
            PathBuf::from("/workspace/demo/master")
        );
    }
}
