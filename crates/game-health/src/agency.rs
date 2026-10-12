//! Health distinguishes participation in care from independent physical action.
use game_core::{AreaId, Error, Location};

#[derive(Clone, Copy, Debug)]
pub struct LocalAgencyFacts {
    pub alive: bool,
    pub health: u64,
    pub in_combat: bool,
}

pub fn local_action_area(
    facts: LocalAgencyFacts,
    agency: bool,
    restricted: impl FnOnce() -> bool,
    independent: impl FnOnce() -> bool,
    location: impl FnOnce() -> Location,
) -> Result<AreaId, Error> {
    if !facts.alive
        || facts.in_combat
        || (agency && (facts.health == 0 || restricted() || !independent()))
    {
        return Err(Error::Unavailable);
    }
    match location() {
        Location::AtArea(area) | Location::StoppedOnRoad { from: area, .. } => Ok(area),
        _ => Err(Error::Unavailable),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_core::{PersonId, SimDuration, SimTime};

    fn facts() -> LocalAgencyFacts {
        LocalAgencyFacts {
            alive: true,
            health: 100,
            in_combat: false,
        }
    }

    fn area(
        facts: LocalAgencyFacts,
        agency: bool,
        restricted: impl FnOnce() -> bool,
        independent: impl FnOnce() -> bool,
        location: &Location,
    ) -> Result<AreaId, Error> {
        local_action_area(facts, agency, restricted, independent, || location.clone())
    }

    #[test]
    fn dependence_and_incapacity_allow_passive_care_but_not_independent_action() {
        let location = Location::AtArea(AreaId(1));
        for (condition, restricted, independent) in [
            (facts(), false, false),
            (
                LocalAgencyFacts {
                    health: 0,
                    ..facts()
                },
                false,
                true,
            ),
            (facts(), true, true),
        ] {
            assert_eq!(
                area(
                    condition,
                    false,
                    || panic!("passive care does not query restrictions"),
                    || panic!("passive care does not query independence"),
                    &location
                ),
                Ok(AreaId(1))
            );
            assert_eq!(
                area(condition, true, || restricted, || independent, &location),
                Err(Error::Unavailable)
            );
        }
        for condition in [
            LocalAgencyFacts {
                alive: false,
                ..facts()
            },
            LocalAgencyFacts {
                in_combat: true,
                ..facts()
            },
        ] {
            assert_eq!(
                local_action_area(
                    condition,
                    false,
                    || panic!("unavailable actor needs no restriction query"),
                    || panic!("unavailable actor needs no independence query"),
                    || panic!("unavailable actor needs no location query"),
                ),
                Err(Error::Unavailable)
            );
            assert_eq!(
                area(condition, false, || false, || true, &location),
                Err(Error::Unavailable)
            );
            assert_eq!(
                area(condition, true, || false, || true, &location),
                Err(Error::Unavailable)
            );
        }
    }

    #[test]
    fn actual_stopped_road_point_is_local_but_travel_and_unresolved_carrying_are_not() {
        let stopped = Location::StoppedOnRoad {
            from: AreaId(1),
            to: AreaId(2),
            progress: SimDuration(10),
            duration: SimDuration(20),
        };
        assert_eq!(
            area(facts(), true, || false, || true, &stopped),
            Ok(AreaId(1))
        );
        let traveling = Location::InTransit {
            from: AreaId(1),
            to: AreaId(2),
            departed: SimTime(0),
            arrives: SimTime(20),
            road_start: None,
        };
        let carried = Location::Carried {
            carrier: PersonId(3),
        };
        for location in [&traveling, &carried] {
            assert_eq!(
                area(facts(), false, || false, || true, location),
                Err(Error::Unavailable)
            );
            assert_eq!(
                area(facts(), true, || false, || true, location),
                Err(Error::Unavailable)
            );
        }
    }
}
