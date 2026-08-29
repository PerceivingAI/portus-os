pub const MAX_RESTART_ATTEMPTS: usize = 3;
pub const RESTART_WINDOW_MS: i64 = 10 * 60 * 1000;
pub const RESTART_BACKOFF_MS: [i64; MAX_RESTART_ATTEMPTS] = [1_000, 5_000, 30_000];
pub const STABLE_RESET_MS: i64 = 60_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RestartBudgetDecision {
    Allowed {
        attempt_number: u16,
        minimum_backoff_ms: i64,
    },
    Exhausted,
}

#[must_use]
pub fn evaluate_restart_budget(
    now_ms: i64,
    prior_attempts_ms: &[i64],
    healthy_since_ms: Option<i64>,
) -> RestartBudgetDecision {
    if healthy_since_ms.is_some_and(|since| now_ms.saturating_sub(since) >= STABLE_RESET_MS) {
        return RestartBudgetDecision::Allowed {
            attempt_number: 1,
            minimum_backoff_ms: RESTART_BACKOFF_MS[0],
        };
    }
    let window_start = now_ms.saturating_sub(RESTART_WINDOW_MS);
    let used = prior_attempts_ms
        .iter()
        .filter(|attempt| **attempt >= window_start && **attempt <= now_ms)
        .count();
    if used >= MAX_RESTART_ATTEMPTS {
        return RestartBudgetDecision::Exhausted;
    }
    RestartBudgetDecision::Allowed {
        attempt_number: u16::try_from(used + 1).unwrap_or(u16::MAX),
        minimum_backoff_ms: RESTART_BACKOFF_MS[used],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restart_budget_exhausts_after_three_attempts_in_ten_minutes() {
        let now = 1_000_000;
        assert_eq!(
            evaluate_restart_budget(now, &[now - 500_000, now - 100_000, now - 1_000], None),
            RestartBudgetDecision::Exhausted
        );
    }

    #[test]
    fn restart_budget_uses_locked_backoff_and_stable_reset() {
        let now = 1_000_000;
        assert_eq!(
            evaluate_restart_budget(now, &[now - 5_000], None),
            RestartBudgetDecision::Allowed {
                attempt_number: 2,
                minimum_backoff_ms: 5_000,
            }
        );
        assert_eq!(
            evaluate_restart_budget(
                now,
                &[now - 5_000, now - 4_000, now - 3_000],
                Some(now - 60_000)
            ),
            RestartBudgetDecision::Allowed {
                attempt_number: 1,
                minimum_backoff_ms: 1_000,
            }
        );
    }
}
