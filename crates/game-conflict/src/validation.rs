//! Contextual owner save checks over typed host facts and canonical components.
use crate::*;
use game_ecs::Store;
use std::collections::BTreeSet;
#[derive(Clone, Debug)]
pub struct SavedPerson {
    pub alive: bool,
    pub location: Location,
    pub combat: Option<EncounterId>,
    pub anchor: PersonId,
}
pub struct SaveFacts {
    pub people: BTreeMap<PersonId, SavedPerson>,
    pub links: BTreeMap<(AreaId, AreaId), SimDuration>,
    pub areas: BTreeSet<AreaId>,
    pub deadlines: Vec<(EncounterId, SimTime)>,
    pub max_health: u64,
}
pub fn validate_absence(store: &Store) -> Result<(), Error> {
    if profiles(store).next().is_some()
        || maps(store).next().is_some()
        || encounters(store).next().is_some()
        || captivities(store).next().is_some()
        || store.iter::<MapRef>().next().is_some()
        || store.iter::<RoadMapRefs>().next().is_some()
        || store.iter::<Participation>().next().is_some()
    {
        return Err(Error::InvalidSave("combat rows without runtime".into()));
    }
    Ok(())
}
impl World {
    pub fn validate(&self, store: &Store, now: SimTime, facts: &SaveFacts) -> Result<(), Error> {
        let fail = || Error::InvalidSave("inconsistent combat state".into());
        if self.rules.round.0 == 0
            || !self.rules.capacity_valid()
            || self.rules.movement > 1_000_000
            || self.rules.reach > 1_000_000
            || self.rules.damage == 0
            || self.rules.incapacitation >= facts.max_health
            || profiles(store).count() != facts.people.len()
            || profiles(store)
                .any(|(id, p)| !facts.people.contains_key(&id) || p.initiative > 1_000_000)
        {
            return Err(fail());
        }
        for (id, map) in maps(store) {
            map.validate().map_err(|_| fail())?;
            if id != map.id
                || map_for_location(store, &map.location, now)?.map(|row| row.id) != Some(id)
            {
                return Err(fail());
            }
            match map.location {
                Location::AtArea(area) => {
                    if map.area != area
                        || !facts.areas.contains(&area)
                        || map.exits.iter().any(|(destination, _)| {
                            !facts.links.contains_key(&(area, *destination))
                        })
                    {
                        return Err(fail());
                    }
                }
                Location::StoppedOnRoad {
                    from,
                    to,
                    progress,
                    duration,
                } => {
                    if map.area != from.min(to)
                        || duration.0 == 0
                        || progress > duration
                        || facts.links.get(&(from, to)) != Some(&duration)
                        || map.exits.len() != 2
                        || map
                            .exits
                            .iter()
                            .any(|(area, _)| *area != from && *area != to)
                        || map.exits[0].0 == map.exits[1].0
                    {
                        return Err(fail());
                    }
                }
                _ => return Err(fail()),
            }
        }
        for (id, row) in store.iter::<MapRef>() {
            if !facts.areas.contains(&AreaId(id.0))
                || map(store, row.map).is_none_or(|map| {
                    game_transport::position_from_location(&map.location, now)
                        .map(|p| p.key())
                        .ok()
                        != Some(game_transport::PositionKey::Area(AreaId(id.0)))
                })
            {
                return Err(fail());
            }
        }
        for (id, refs) in store.iter::<RoadMapRefs>() {
            for (key, map_id) in &refs.maps {
                if map(store, *map_id).is_none_or(|map| {
                    game_transport::position_from_location(&map.location, now)
                        .map(|p| p.key())
                        .ok()
                        != Some(game_transport::PositionKey::Road {
                            from: AreaId(id.0),
                            to: key.to,
                            numerator: key.numerator,
                            denominator: key.denominator,
                        })
                }) {
                    return Err(fail());
                }
            }
        }
        let mut deadlines = BTreeSet::new();
        for (id, at) in &facts.deadlines {
            if !encounter(store, *id).is_some_and(|e| e.active && e.ends == *at)
                || !deadlines.insert(*id)
            {
                return Err(fail());
            }
        }
        let mut active = BTreeSet::new();
        for (id, e) in encounters(store) {
            let map = map(store, e.map).ok_or_else(fail)?;
            let place = game_transport::position_from_location(&e.location, now)?.key();
            if id != e.id
                || !(2..=self.rules.encounter_limit).contains(&e.fighters.len())
                || e.area != map.area
                || !matches!(
                    e.location,
                    Location::AtArea(_) | Location::StoppedOnRoad { .. }
                )
                || game_transport::position_from_location(&map.location, now)?.key() != place
                || e.started > now
                || e.ends.0.checked_sub(e.started.0) != Some(self.rules.round.0)
                || e.cursor > e.order.len()
                || e.order.iter().collect::<BTreeSet<_>>().len() != e.order.len()
                || e.order.iter().any(|a| !e.fighters.contains_key(a))
                || e.active != deadlines.contains(&id)
            {
                return Err(fail());
            }
            for (actor, fighter) in &e.fighters {
                let person = facts.people.get(actor).ok_or_else(fail)?;
                if *actor != fighter.actor
                    || fighter.joins_round > e.round.saturating_add(1)
                    || (fighter.joins_round > e.round && e.order.contains(actor))
                    || !map.contains(fighter.position)
                    || !fighter.search.valid(map)
                    || fighter
                        .known
                        .iter()
                        .any(|(id, p)| !e.fighters.contains_key(id) || !map.contains(*p))
                {
                    return Err(fail());
                }
                if e.active
                    && !fighter.withdrawn
                    && person.anchor != *actor
                    && e.fighters
                        .get(&person.anchor)
                        .is_none_or(|parent| parent.position != fighter.position)
                {
                    return Err(fail());
                }
                if e.active
                    && (!fighter.withdrawn || fighter.retreat.is_some())
                    && person.alive
                    && (!active.insert(*actor)
                        || person.combat != Some(id)
                        || game_transport::position_from_location(&person.location, now)?.key()
                            != place)
                {
                    return Err(fail());
                }
            }
        }
        for (actor, person) in &facts.people {
            if person.combat.is_some() != active.contains(actor) {
                return Err(fail());
            }
        }
        for (id, participation) in store.iter::<Participation>() {
            if !facts.people.contains_key(&PersonId(id.0))
                || encounter(store, participation.encounter)
                    .is_none_or(|e| !e.fighters.contains_key(&PersonId(id.0)))
            {
                return Err(fail());
            }
        }
        for (actor, c) in captivities(store) {
            if actor != c.actor
                || !facts.people.contains_key(&actor)
                || !facts.people.contains_key(&c.captor)
                || map(store, c.map).is_none_or(|m| m.area != c.area || !m.contains(c.position))
                || !game_cargo::inventory(store, c.binding).is_some_and(|i| {
                    i.good == Good::Binding
                        && i.quantity.0 == 1_000
                        && i.location == game_cargo::InventoryLocation::Carried(actor)
                })
            {
                return Err(fail());
            }
        }
        Ok(())
    }
}
