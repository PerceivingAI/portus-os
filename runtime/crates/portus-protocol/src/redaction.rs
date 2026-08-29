use std::fmt;

/// In-memory wrapper for values that must never be exposed through ordinary
/// `Debug` or `Display` formatting.
///
/// `Redacted<T>` intentionally does not implement serde traits. Protected or
/// otherwise sensitive values must cross only their explicitly owned boundary,
/// never generic Portus serialization by convenience.
pub struct Redacted<T>(T);

impl<T> Redacted<T> {
    #[must_use]
    pub const fn new(value: T) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn expose_ref(&self) -> &T {
        &self.0
    }

    #[must_use]
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> fmt::Debug for Redacted<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Redacted([REDACTED])")
    }
}

impl<T> fmt::Display for Redacted<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacted_values_do_not_format_inner_content() {
        let value = Redacted::new("super-secret-value".to_owned());
        assert_eq!(format!("{value}"), "[REDACTED]");
        assert_eq!(format!("{value:?}"), "Redacted([REDACTED])");
        assert!(!format!("{value:?}").contains(value.expose_ref()));
    }
}
