//! Attempt telemetry. The macro erases its body in ordinary builds.
/// Evaluate ordered owner predicates exactly once, recording only the first
/// failing guard in profile builds. Ordinary builds retain the boolean checks.
#[macro_export]
macro_rules! interaction_checks {
    ($($condition:expr => $key:literal),+ $(,)?) => {
        true $(&& {
            let valid = $condition;
            $crate::interaction_profile!({
                if !valid { $crate::count($key, 1); }
            });
            valid
        })+
    };
}
#[cfg(feature = "interaction-profile")]
#[doc(hidden)]
pub fn interaction_enabled() -> bool {
    *super::ENABLED.get_or_init(|| std::env::var_os("LEGACY_PROFILE").is_some())
}
/// Add a quantity to `snapshot()[key].calls`, without measuring time.
/// Counts are process-local attempts and are not undone by simulation rollback.
#[cfg(feature = "interaction-profile")]
pub fn count(key: &'static str, amount: u64) {
    if interaction_enabled() {
        let mut data = super::DATA.get_or_init(Default::default).lock().unwrap();
        let value = data.entry(key).or_default();
        value.calls = value.calls.saturating_add(amount);
    }
}

/// Bounded, stable proposal-family counters; identifiers never enter metric keys.
#[cfg(feature = "interaction-profile")]
pub fn ordered_proposal(key: &str, continues: bool) {
    count(
        if continues {
            "interaction.ordered.continuation"
        } else {
            "interaction.ordered.new"
        },
        1,
    );
    macro_rules! families {
        ($($prefix:literal),* $(,)?) => {
            $(if key == $prefix || key.starts_with(concat!($prefix, ":")) {
                count(concat!("interaction.ordered.family.", $prefix), 1);
                return;
            })*
        };
    }
    families!(
        "information:answer",
        "information:decline",
        "information:ask",
        "information:read",
        "food",
        "care",
        "aid",
        "inquiry",
        "company",
        "combat",
        "family",
        "parenthood",
        "memory",
        "literacy",
        "journey",
        "rest",
        "wait",
        "work",
        "exchange",
        "garden",
        "cultivation",
        "storage",
        "community",
        "institution",
        "credit",
        "claim",
        "dispute",
        "justice",
        "animal",
        "heat",
        "clothing",
        "mechanics",
        "idea",
        "beauty",
        "invocation"
    );
    count("interaction.ordered.family.other", 1);
}

#[cfg(feature = "interaction-profile")]
pub fn interaction_meal(self_meal: bool, accepted: Option<bool>) {
    count(
        match (self_meal, accepted) {
            (true, None) => "interaction.candidate.self",
            (false, None) => "interaction.candidate.care",
            (true, Some(true)) => "interaction.wave.accepted.self",
            (false, Some(true)) => "interaction.wave.accepted.care",
            (true, Some(false)) => "interaction.wave.refused.self",
            (false, Some(false)) => "interaction.wave.refused.care",
        },
        1,
    );
}

/// Execute a diagnostic block only in profile builds with LEGACY_PROFILE set.
/// The batch form records the actual execution branch and number of owner rows.
#[cfg(feature = "interaction-profile")]
#[macro_export]
macro_rules! interaction_profile {
    (count $key:expr, $amount:expr) => {
        $crate::interaction_profile!({ $crate::count($key, $amount); });
    };
    (size $prefix:literal, $size:expr) => {
        $crate::interaction_profile!({
            $crate::count(match $size {
                0 => concat!($prefix, ".0"),
                1 => concat!($prefix, ".1"),
                2..=3 => concat!($prefix, ".2-3"),
                4..=7 => concat!($prefix, ".4-7"),
                8..=15 => concat!($prefix, ".8-15"),
                _ => concat!($prefix, ".16+"),
            }, 1);
        });
    };
    (batch $owner:literal, $rows:expr, $parallel:expr) => {
        $crate::interaction_profile!({
            let (calls, rows) = if $parallel {
                (concat!("interaction.owner.", $owner, ".parallel.calls"),
                 concat!("interaction.owner.", $owner, ".parallel.rows"))
            } else {
                (concat!("interaction.owner.", $owner, ".serial.calls"),
                 concat!("interaction.owner.", $owner, ".serial.rows"))
            };
            $crate::count(calls, 1);
            $crate::count(rows, $rows as u64);
        });
    };
    ($($body:tt)*) => {{
        if $crate::interaction_enabled() { $($body)* }
    }};
}
#[cfg(not(feature = "interaction-profile"))]
#[macro_export]
macro_rules! interaction_profile {
    ($($body:tt)*) => {};
}

#[cfg(test)]
mod tests {
    #[test]
    fn guard_checks_keep_short_circuit_order_in_every_build() {
        #[cfg(feature = "interaction-profile")]
        let _ = crate::ENABLED.set(true);
        let called = std::cell::Cell::new(0);
        assert!(!crate::interaction_checks!(
            { called.set(1); true } => "interaction.test.guard.first",
            { called.set(2); false } => "interaction.test.guard.second",
            { called.set(3); true } => "interaction.test.guard.third",
        ));
        assert_eq!(called.get(), 2);
    }
    #[test]
    #[cfg(not(feature = "interaction-profile"))]
    fn ordinary_builds_do_not_evaluate_diagnostic_arguments() {
        let called = std::cell::Cell::new(false);
        crate::interaction_profile!({
            called.set(true);
        });
        crate::interaction_profile!(batch "test", { called.set(true); 7 }, true);
        assert!(!called.get());
    }
    #[test]
    #[cfg(feature = "interaction-profile")]
    fn profile_counts_quantities_and_actual_batch_branches_without_timers() {
        let _ = crate::ENABLED.set(true);
        crate::interaction_profile!({
            crate::count("interaction.test.candidates", 5);
        });
        crate::interaction_profile!(batch "test", 3, true);
        crate::interaction_profile!(batch "test", 1, false);
        crate::ordered_proposal("information:ask:123:456", false);
        crate::ordered_proposal("information:ask:789:012", true);
        crate::ordered_proposal("unknown:123", false);
        let snapshot = crate::snapshot();
        assert_eq!(snapshot["interaction.test.candidates"].calls, 5);
        assert_eq!(snapshot["interaction.owner.test.parallel.calls"].calls, 1);
        assert_eq!(snapshot["interaction.owner.test.parallel.rows"].calls, 3);
        assert_eq!(snapshot["interaction.owner.test.serial.rows"].calls, 1);
        assert_eq!(snapshot["interaction.test.candidates"].nanoseconds, 0);
        assert_eq!(
            snapshot["interaction.ordered.family.information:ask"].calls,
            2
        );
        assert_eq!(snapshot["interaction.ordered.family.other"].calls, 1);
    }
}
