//! Triage uses the observer's recorded interpretation, never remote bodily truth.
use super::*;
use game_signals::Perception;

pub enum SignalReaction {
    Ignore,
    Changed,
    Urgent,
}
fn urgent_change(before: Option<&Perception>, after: &Perception, threshold: u64) -> bool {
    let urgent = match after.guessed_need.as_deref() {
        Some("food") => after.estimated_need >= threshold,
        Some("pain") => after.estimated_need > 0,
        _ => false,
    };
    urgent
        && before.is_none_or(|old| {
            old.guessed_need != after.guessed_need
                || old.estimated_need < after.estimated_need
                || old.area != after.area
        })
}
pub fn notice_prioritized(
    tx: &mut Transaction<'_>,
    observer: PersonId,
    person: PersonId,
    changes_only: bool,
    threshold: u64,
) -> SignalReaction {
    let before = game_signals::perception(tx, observer, person).cloned();
    match tx.attempt(game_communication::Notice { observer, person }) {
        Ok(after) if urgent_change(before.as_ref(), &after, threshold) => SignalReaction::Urgent,
        Ok(after) if changes_only && !super::food_changed(before.as_ref(), &after) => {
            SignalReaction::Ignore
        }
        _ => SignalReaction::Changed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn food(need: u64) -> Perception {
        Perception {
            cue: "reaching-mouth".into(),
            guessed_need: Some("food".into()),
            estimated_need: need,
            at: SimTime(1),
            last_fed: None,
            food: None,
            appetite: 1000,
            area: Some(AreaId(1)),
        }
    }
    #[test]
    fn critical_new_or_worsening_cues_are_urgent_but_timestamps_and_repetition_are_not() {
        let old = food(100);
        assert!(urgent_change(None, &old, 90));
        let mut after = old.clone();
        after.at = SimTime(2);
        assert!(!urgent_change(Some(&old), &after, 90));
        after.estimated_need = 101;
        assert!(urgent_change(Some(&old), &after, 90));
        after.estimated_need = 80;
        assert!(!urgent_change(Some(&old), &after, 90));
        assert!(!urgent_change(None, &after, 90));
        after.guessed_need = Some("pain".into());
        after.estimated_need = 5;
        assert!(urgent_change(Some(&old), &after, 90));
        assert!(!urgent_change(Some(&after), &after, 90));
        let mut elsewhere = after.clone();
        elsewhere.area = Some(AreaId(2));
        assert!(urgent_change(Some(&after), &elsewhere, 90));
    }
}
