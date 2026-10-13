//! Pure policy for ordering and closing physical activity completions.

use crate::ActivityKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompletionGroup {
    Early,
    Production,
    Late,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompletionDirective {
    Complete,
    Interrupt,
    Skip,
}

/// Classify a due physical completion using the same activity families as dispatch.
pub fn classify_completion(
    physical_available: bool,
    activity: Option<ActivityKind>,
) -> CompletionGroup {
    if physical_available
        && matches!(
            activity,
            Some(ActivityKind::Gather | ActivityKind::Cultivate | ActivityKind::Construct)
        )
    {
        CompletionGroup::Production
    } else if physical_available
        && matches!(
            activity,
            Some(
                ActivityKind::Escape
                    | ActivityKind::Restrain
                    | ActivityKind::Lift
                    | ActivityKind::RealmTalk
            )
        )
    {
        CompletionGroup::Late
    } else {
        CompletionGroup::Early
    }
}

/// Decide whether a captured completion should close after earlier groups run.
pub fn closing_completion(
    group: CompletionGroup,
    physical_available: bool,
    current_activity: Option<ActivityKind>,
    equipped: bool,
) -> CompletionDirective {
    match group {
        CompletionGroup::Production => {
            if equipped {
                CompletionDirective::Complete
            } else {
                CompletionDirective::Interrupt
            }
        }
        CompletionGroup::Late
            if physical_available
                && matches!(
                    current_activity,
                    Some(
                        ActivityKind::Escape
                            | ActivityKind::Restrain
                            | ActivityKind::Lift
                            | ActivityKind::RealmTalk
                    )
                ) =>
        {
            CompletionDirective::Complete
        }
        CompletionGroup::Early | CompletionGroup::Late => CompletionDirective::Skip,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_physical_work_and_late_activity_families() {
        for activity in [
            ActivityKind::Gather,
            ActivityKind::Cultivate,
            ActivityKind::Construct,
        ] {
            assert_eq!(
                classify_completion(true, Some(activity)),
                CompletionGroup::Production
            );
        }

        for activity in [
            ActivityKind::Escape,
            ActivityKind::Restrain,
            ActivityKind::Lift,
            ActivityKind::RealmTalk,
        ] {
            assert_eq!(
                classify_completion(true, Some(activity)),
                CompletionGroup::Late
            );
        }
    }

    #[test]
    fn unavailable_or_non_family_activity_is_early() {
        for activity in [
            ActivityKind::Other,
            ActivityKind::Idle,
            ActivityKind::Eat,
            ActivityKind::Rest,
            ActivityKind::Work,
            ActivityKind::Combat,
        ] {
            assert_eq!(
                classify_completion(true, Some(activity)),
                CompletionGroup::Early
            );
        }
        assert_eq!(
            classify_completion(false, Some(ActivityKind::Gather)),
            CompletionGroup::Early
        );
        assert_eq!(classify_completion(true, None), CompletionGroup::Early);
    }

    #[test]
    fn closing_preserves_production_rules_and_rechecks_late_activity() {
        assert_eq!(
            closing_completion(CompletionGroup::Production, true, None, true),
            CompletionDirective::Complete
        );
        assert_eq!(
            closing_completion(
                CompletionGroup::Production,
                false,
                Some(ActivityKind::Idle),
                false
            ),
            CompletionDirective::Interrupt
        );

        for activity in [
            ActivityKind::Escape,
            ActivityKind::Restrain,
            ActivityKind::Lift,
            ActivityKind::RealmTalk,
        ] {
            assert_eq!(
                closing_completion(CompletionGroup::Late, true, Some(activity), false),
                CompletionDirective::Complete
            );
        }
        assert_eq!(
            closing_completion(CompletionGroup::Late, true, Some(ActivityKind::Idle), false),
            CompletionDirective::Skip
        );
        assert_eq!(
            closing_completion(
                CompletionGroup::Late,
                false,
                Some(ActivityKind::Escape),
                false
            ),
            CompletionDirective::Skip
        );
        assert_eq!(
            closing_completion(
                CompletionGroup::Early,
                true,
                Some(ActivityKind::Escape),
                true
            ),
            CompletionDirective::Skip
        );
    }
}
