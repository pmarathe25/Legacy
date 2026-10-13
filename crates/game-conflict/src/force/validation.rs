use super::*;
use game_cargo::{Inventory, InventoryLocation};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Activity {
    Other,
    Escape {
        tool: InventoryId,
        completes: SimTime,
    },
    Unbind {
        completes: SimTime,
    },
    Restrain {
        target: PersonId,
        binding: InventoryId,
        completes: SimTime,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationPerson {
    pub alive: bool,
    pub location: Location,
    pub activity: Activity,
}
pub trait References {
    fn person(&self, actor: PersonId) -> Option<ValidationPerson>;
    fn area_exists(&self, area: AreaId) -> bool;
    fn confined(&self, target: PersonId) -> bool;
    fn same_encounter(&self, target: PersonId, actor: PersonId) -> bool;
    fn due(&self, actor: PersonId, completes: SimTime) -> bool;
    fn activities(&self) -> impl Iterator<Item = (PersonId, Activity)>;
}
pub fn validate_absent(store: &Store) -> Result<(), Error> {
    if store.count::<Yielded>() + store.count::<Restraint>() + store.count::<Escape>() != 0 {
        return Err(Error::InvalidSave("orphan force components".into()));
    }
    Ok(())
}
pub fn validate_absent_activities(
    activities: impl IntoIterator<Item = Activity>,
) -> Result<(), Error> {
    if activities
        .into_iter()
        .any(|activity| !matches!(activity, Activity::Other))
    {
        return Err(Error::InvalidSave("orphan force activity".into()));
    }
    Ok(())
}
impl Runtime {
    pub fn validate(
        &self,
        store: &Store,
        references: &impl References,
        now: SimTime,
        next_event: u64,
    ) -> Result<(), Error> {
        let fail = || Error::InvalidSave("invalid confinement attempt or food supply".into());
        if self.rules.restraint_time.0 == 0
            || self.rules.escape_time.0 == 0
            || self.last_event >= next_event
        {
            return Err(fail());
        }
        for (actor, row) in yields(store) {
            if references.person(actor).is_none()
                || row.captors.is_empty()
                || row.captors.contains(&actor)
                || row
                    .captors
                    .iter()
                    .any(|actor| references.person(*actor).is_none())
                || row.at > now
                || !references.area_exists(row.area)
            {
                return Err(fail());
            }
        }
        for (actor, row) in restraints(store) {
            let person = references.person(actor).ok_or_else(fail)?;
            let target = references.person(row.target).ok_or_else(fail)?;
            if actor == row.target
                || row.completes <= now
                || !person.alive
                || person.location != row.location
                || target.location != row.location
                || store
                    .get::<Inventory>(StableId(row.binding.0))
                    .is_none_or(|stock| {
                        stock.good != Good::Binding
                            || stock.location != InventoryLocation::Carried(actor)
                    })
                || person.activity
                    != (Activity::Restrain {
                        target: row.target,
                        binding: row.binding,
                        completes: row.completes,
                    })
                || !references.due(actor, row.completes)
            {
                return Err(fail());
            }
        }
        for (target, row) in escapes(store) {
            if target != row.target
                || references.person(target).is_none()
                || references.person(row.actor).is_none()
                || !references.confined(target)
                || row.began > now
                || row.began >= row.completes
            {
                return Err(fail());
            }
            if row.tool.is_some_and(|tool| {
                store
                    .get::<Inventory>(StableId(tool.0))
                    .is_none_or(|stock| {
                        stock.good != Good::Tool
                            || stock.location != InventoryLocation::Carried(row.actor)
                    })
            }) {
                return Err(fail());
            }
            if row.contested {
                if !references.same_encounter(target, row.actor) {
                    return Err(fail());
                }
            } else {
                if row.completes <= now {
                    return Err(fail());
                }
                let activity = references.person(row.actor).ok_or_else(fail)?.activity;
                if !matches!(activity, Activity::Escape { completes, .. } | Activity::Unbind { completes } if completes == row.completes)
                {
                    return Err(fail());
                }
            }
        }
        for (actor, activity) in references.activities() {
            match activity {
                Activity::Escape { .. }
                    if !escape(store, actor)
                        .is_some_and(|row| row.actor == actor && !row.contested) =>
                {
                    return Err(fail());
                }
                Activity::Restrain { .. } if restraint(store, actor).is_none() => {
                    return Err(fail());
                }
                _ => {}
            }
        }
        Ok(())
    }
}
