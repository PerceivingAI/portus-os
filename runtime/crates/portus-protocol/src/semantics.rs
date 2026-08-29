use serde::{Deserialize, Serialize};
use std::fmt;

macro_rules! string_enum {
    ($name:ident { $($variant:ident => $value:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $value),+
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

string_enum!(HealthState {
    Healthy => "healthy",
    Degraded => "degraded",
    Unavailable => "unavailable",
    Unknown => "unknown",
});

string_enum!(RecoveryDisposition {
    Observe => "observe",
    Reconcile => "reconcile",
    Restart => "restart",
    Repair => "repair",
    AdministratorRequired => "administrator_required",
    Terminal => "terminal",
});

string_enum!(Freshness {
    Live => "live",
    Recent => "recent",
    Stale => "stale",
    Unavailable => "unavailable",
    Historical => "historical",
});

string_enum!(EvidenceStrength {
    Authoritative => "authoritative",
    Strong => "strong",
    Heuristic => "heuristic",
});

string_enum!(SemanticErrorCode {
    InvalidArgument => "invalid_argument",
    InvalidRequest => "invalid_request",
    UnsupportedOutputMode => "unsupported_output_mode",
    DaemonUnavailable => "daemon_unavailable",
    ProtocolError => "protocol_error",
    IncompatibleProtocol => "incompatible_protocol",
    NotFound => "not_found",
    StaleResource => "stale_resource",
    PreconditionFailed => "precondition_failed",
    Conflict => "conflict",
    PermissionDenied => "permission_denied",
    ApprovalRequired => "approval_required",
    Unavailable => "unavailable",
    ProviderUnavailable => "provider_unavailable",
    SourceUnavailable => "source_unavailable",
    Unsupported => "unsupported",
    Timeout => "timeout",
    Cancelled => "cancelled",
    Interrupted => "interrupted",
    Internal => "internal",
});

impl SemanticErrorCode {
    #[must_use]
    pub const fn exit_code(self) -> u8 {
        match self {
            Self::InvalidArgument | Self::InvalidRequest | Self::UnsupportedOutputMode => 2,
            Self::DaemonUnavailable | Self::ProtocolError | Self::IncompatibleProtocol => 3,
            Self::NotFound => 4,
            Self::PermissionDenied | Self::ApprovalRequired => 5,
            Self::StaleResource | Self::PreconditionFailed | Self::Conflict => 6,
            Self::Unavailable
            | Self::ProviderUnavailable
            | Self::SourceUnavailable
            | Self::Unsupported => 7,
            Self::Timeout => 8,
            Self::Cancelled | Self::Interrupted => 9,
            Self::Internal => 10,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enums_use_locked_wire_values() {
        assert_eq!(
            serde_json::to_string(&HealthState::Degraded).unwrap(),
            r#""degraded""#
        );
        assert_eq!(
            serde_json::to_string(&RecoveryDisposition::AdministratorRequired).unwrap(),
            r#""administrator_required""#
        );
        assert_eq!(
            serde_json::to_string(&Freshness::Historical).unwrap(),
            r#""historical""#
        );
        assert_eq!(
            serde_json::to_string(&EvidenceStrength::Authoritative).unwrap(),
            r#""authoritative""#
        );
        assert_eq!(
            serde_json::to_string(&SemanticErrorCode::PreconditionFailed).unwrap(),
            r#""precondition_failed""#
        );
    }

    #[test]
    fn semantic_errors_have_stable_exit_families() {
        assert_eq!(SemanticErrorCode::InvalidRequest.exit_code(), 2);
        assert_eq!(SemanticErrorCode::IncompatibleProtocol.exit_code(), 3);
        assert_eq!(SemanticErrorCode::NotFound.exit_code(), 4);
        assert_eq!(SemanticErrorCode::ApprovalRequired.exit_code(), 5);
        assert_eq!(SemanticErrorCode::StaleResource.exit_code(), 6);
        assert_eq!(SemanticErrorCode::ProviderUnavailable.exit_code(), 7);
        assert_eq!(SemanticErrorCode::Timeout.exit_code(), 8);
        assert_eq!(SemanticErrorCode::Interrupted.exit_code(), 9);
        assert_eq!(SemanticErrorCode::Internal.exit_code(), 10);
    }
}
