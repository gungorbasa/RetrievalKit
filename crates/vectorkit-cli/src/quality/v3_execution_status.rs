use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FailureReason {
    GenerationMismatch,
    StaleSelection,
    PersistenceMismatch,
    ReloadMismatch,
    NonDeterministicRanking,
    ContractViolation,
}

impl FailureReason {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::GenerationMismatch => "generation_mismatch",
            Self::StaleSelection => "stale_selection",
            Self::PersistenceMismatch => "persistence_mismatch",
            Self::ReloadMismatch => "reload_mismatch",
            Self::NonDeterministicRanking => "non_deterministic_ranking",
            Self::ContractViolation => "contract_violation",
        }
    }

    const fn precedence(self) -> u8 {
        match self {
            Self::GenerationMismatch => 6,
            Self::StaleSelection => 5,
            Self::PersistenceMismatch => 4,
            Self::ReloadMismatch => 3,
            Self::NonDeterministicRanking => 2,
            Self::ContractViolation => 1,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct ExecutionFailures {
    runs: BTreeMap<String, FailureReason>,
    queries: BTreeMap<(String, String), FailureReason>,
}

impl ExecutionFailures {
    pub(super) fn run(&mut self, run_id: impl Into<String>, reason: FailureReason) {
        retain_preferred(&mut self.runs, run_id.into(), reason);
    }

    pub(super) fn query(
        &mut self,
        run_id: impl Into<String>,
        query_id: impl Into<String>,
        reason: FailureReason,
    ) {
        retain_preferred(&mut self.queries, (run_id.into(), query_id.into()), reason);
    }

    pub(super) fn reason_for(&self, run_id: &str, query_id: &str) -> Option<FailureReason> {
        self.runs.get(run_id).copied().or_else(|| {
            self.queries
                .get(&(run_id.to_owned(), query_id.to_owned()))
                .copied()
        })
    }

    pub(super) fn run_reason(&self, run_id: &str) -> Option<FailureReason> {
        self.runs.get(run_id).copied()
    }

    pub(super) fn run_is_invalid(&self, run_id: &str) -> bool {
        self.runs.contains_key(run_id)
            || self
                .queries
                .keys()
                .any(|(candidate, _)| candidate == run_id)
    }

    pub(super) fn is_empty(&self) -> bool {
        self.runs.is_empty() && self.queries.is_empty()
    }
}

pub(super) fn classify_query_failure(message: &str) -> (FailureReason, bool) {
    let normalized = message.to_ascii_lowercase();
    for (token, reason) in [
        ("generation_mismatch", FailureReason::GenerationMismatch),
        ("generation mismatch", FailureReason::GenerationMismatch),
        ("stale_selection", FailureReason::StaleSelection),
        ("stalegeneration", FailureReason::StaleSelection),
        ("stale generation", FailureReason::StaleSelection),
        ("persistence_mismatch", FailureReason::PersistenceMismatch),
        ("reload_mismatch", FailureReason::ReloadMismatch),
        (
            "non_deterministic_ranking",
            FailureReason::NonDeterministicRanking,
        ),
    ] {
        if normalized.contains(token) {
            return (reason, true);
        }
    }
    (FailureReason::ContractViolation, false)
}

fn retain_preferred<K: Ord>(
    values: &mut BTreeMap<K, FailureReason>,
    key: K,
    reason: FailureReason,
) {
    match values.get_mut(&key) {
        Some(current) if current.precedence() < reason.precedence() => *current = reason,
        Some(_) => {}
        None => {
            values.insert(key, reason);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_wide_failure_overrides_query_local_reason_regardless_of_precedence() {
        let mut failures = ExecutionFailures::default();
        failures.query("run", "query", FailureReason::GenerationMismatch);
        failures.run("run", FailureReason::ContractViolation);
        assert_eq!(
            failures.reason_for("run", "query"),
            Some(FailureReason::ContractViolation)
        );
    }

    #[test]
    fn reasons_use_contract_precedence_within_the_same_scope() {
        let mut failures = ExecutionFailures::default();
        failures.run("run", FailureReason::ContractViolation);
        failures.run("run", FailureReason::ReloadMismatch);
        failures.run("run", FailureReason::GenerationMismatch);
        failures.run("run", FailureReason::StaleSelection);
        assert_eq!(
            failures.run_reason("run"),
            Some(FailureReason::GenerationMismatch)
        );
    }
}
