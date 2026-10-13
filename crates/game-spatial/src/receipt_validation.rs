//! Timing of saved movement receipts.
use game_core::{Error, SimTime};
pub fn validate_departure(at: SimTime, arrives: SimTime) -> Result<(), Error> {
    if arrives > at {
        Ok(())
    } else {
        Err(Error::InvalidSave("inconsistent committed state".into()))
    }
}
pub fn validate_link_rows(
    links: impl IntoIterator<Item = (game_core::AreaId, game_core::SimDuration)>,
    areas: &std::collections::BTreeSet<game_core::AreaId>,
) -> Result<(), Error> {
    let mut destinations = std::collections::BTreeSet::new();
    for (destination, duration) in links {
        if duration.0 == 0 || !areas.contains(&destination) || !destinations.insert(destination) {
            return Err(Error::InvalidSave("inconsistent committed state".into()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saved_links_keep_distinct_known_destinations_with_positive_duration() {
        use game_core::{AreaId, SimDuration};
        let areas = std::collections::BTreeSet::from([AreaId(1), AreaId(2)]);
        assert!(validate_link_rows([(AreaId(2), SimDuration(1))], &areas).is_ok());
        assert!(
            validate_link_rows(
                [(AreaId(2), SimDuration(1)), (AreaId(2), SimDuration(2))],
                &areas
            )
            .is_err()
        );
        assert!(validate_link_rows([(AreaId(2), SimDuration(0))], &areas).is_err());
        assert!(validate_link_rows([(AreaId(3), SimDuration(1))], &areas).is_err());
    }
    #[test]
    fn departure_requires_future_arrival() {
        assert!(validate_departure(SimTime(5), SimTime(6)).is_ok());
        assert!(validate_departure(SimTime(5), SimTime(5)).is_err());
        assert!(validate_departure(SimTime(5), SimTime(4)).is_err());
    }
}
