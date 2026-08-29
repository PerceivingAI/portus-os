use serde::{Deserialize, Serialize};
use std::fmt;

/// Kernel-derived effective local principal used for PortusOS authorization.
///
/// Usernames are presentation metadata and are intentionally not part of this
/// identity type. Runtime transports derive these values from peer credentials.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Principal {
    uid: u32,
    gid: u32,
}

impl Principal {
    #[must_use]
    pub const fn new(uid: u32, gid: u32) -> Self {
        Self { uid, gid }
    }

    #[must_use]
    pub const fn uid(self) -> u32 {
        self.uid
    }

    #[must_use]
    pub const fn gid(self) -> u32 {
        self.gid
    }

    #[must_use]
    pub const fn is_root(self) -> bool {
        self.uid == 0
    }
}

impl fmt::Debug for Principal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Principal")
            .field("uid", &self.uid)
            .field("gid", &self.gid)
            .finish()
    }
}

impl fmt::Display for Principal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "uid:{} gid:{}", self.uid, self.gid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn principal_is_numeric_and_username_free() {
        let principal = Principal::new(1000, 100);
        let json = serde_json::to_string(&principal).unwrap();
        assert_eq!(json, r#"{"uid":1000,"gid":100}"#);
        assert!(!json.contains("user"));
        assert!(!principal.is_root());
        assert!(Principal::new(0, 0).is_root());
    }
}
