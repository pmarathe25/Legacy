//! Dismissal retains only routes supported by the current person's knowledge.
use game_core::{AreaId, Location, SimDuration};
use std::collections::BTreeMap;

pub fn retained_route_after_dismissal(
    current: &BTreeMap<AreaId, BTreeMap<AreaId, SimDuration>>,
    location: &Location,
    pending: &[AreaId],
) -> Option<Vec<AreaId>> {
    let destination = pending.last().copied()?;
    let origin = match location {
        Location::AtArea(area) => *area,
        Location::InTransit { to, .. } => *to,
        Location::StoppedOnRoad { .. } | Location::Carried { .. } => return None,
    };
    Some(
        game_characters::decisions::known_path(current, origin, destination)
            .map_or_else(|_| Vec::new(), |(path, _)| path),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dismissal_replans_only_future_legs_from_current_knowledge() {
        let current = BTreeMap::from([(AreaId(2), BTreeMap::from([(AreaId(3), SimDuration(50))]))]);
        assert_eq!(
            retained_route_after_dismissal(
                &current,
                &Location::AtArea(AreaId(1)),
                &[AreaId(2), AreaId(3)]
            ),
            Some(Vec::new())
        );
        assert_eq!(
            retained_route_after_dismissal(&current, &Location::AtArea(AreaId(2)), &[AreaId(3)]),
            Some(vec![AreaId(3)])
        );
        assert_eq!(
            retained_route_after_dismissal(
                &current,
                &Location::Carried {
                    carrier: game_core::PersonId(1)
                },
                &[AreaId(3)]
            ),
            None
        );
        assert_eq!(
            retained_route_after_dismissal(&current, &Location::AtArea(AreaId(2)), &[]),
            None
        );
    }
}
