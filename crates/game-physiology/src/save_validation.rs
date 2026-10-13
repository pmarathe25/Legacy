//! Persisted physical activity invariants over foreign factual ports.
use crate::{ActivityKind, validate_location_activity};
use game_core::*;
use std::collections::BTreeSet;

#[derive(Clone, Debug)]
pub enum SavedActivity<Operation> {
    ArmyApproach,
    ArmyTalk,
    RealmTalk,
    Lift,
    Social,
    Instruction,
    Practice,
    EnforcementTalk,
    HealthWork,
    Gather,
    MissionTalk,
    Escape,
    Restrain,
    ReportAgreement,
    WritingMemory,
    ReadingMemory,
    TeachingMemory,
    MovingRecord,
    Construct,
    Lesson,
    Take,
    ReportIncident,
    Cultivate {
        field: SiteId,
        operation: Operation,
        completes: SimTime,
    },
    Comfort {
        child: PersonId,
    },
    Idle,
    Rest,
    Eat {
        source: InventoryId,
    },
    Load {
        source: InventoryId,
        destination: InventoryId,
        quantity: Quantity,
        completes: SimTime,
    },
    Work,
    Tell {
        recipient: PersonId,
        inventory: InventoryId,
        observed_at: SimTime,
    },
    Conceive {
        pregnancy: PregnancyId,
        completes: SimTime,
    },
    Care {
        child: PersonId,
        source: InventoryId,
    },
    Combat,
    Unbind {
        target: PersonId,
    },
    Express {
        completes: SimTime,
    },
    Play {
        completes: SimTime,
    },
}
impl<Operation> SavedActivity<Operation> {
    pub fn requires_deadline(&self) -> bool {
        matches!(
            self,
            Self::RealmTalk
                | Self::Lift
                | Self::Social
                | Self::Instruction
                | Self::Practice
                | Self::EnforcementTalk
                | Self::Gather
                | Self::ReportAgreement
                | Self::Escape
                | Self::Restrain
                | Self::Eat { .. }
                | Self::Load { .. }
                | Self::Tell { .. }
                | Self::Care { .. }
                | Self::Conceive { .. }
                | Self::Unbind { .. }
                | Self::Comfort { .. }
                | Self::Express { .. }
                | Self::Play { .. }
                | Self::Construct
                | Self::Lesson
                | Self::Take
                | Self::ReportIncident
                | Self::Cultivate { .. }
        )
    }
    fn kind(&self) -> ActivityKind {
        match self {
            Self::Idle => ActivityKind::Idle,
            Self::Rest => ActivityKind::Rest,
            Self::Eat { .. } => ActivityKind::Eat,
            Self::Care { .. } => ActivityKind::Care,
            Self::Comfort { .. } => ActivityKind::Comfort,
            Self::Express { .. } => ActivityKind::Express,
            Self::Lift => ActivityKind::Lift,
            Self::HealthWork => ActivityKind::HealthWork,
            Self::Unbind { .. } => ActivityKind::Unbind,
            Self::Escape => ActivityKind::Escape,
            Self::Restrain => ActivityKind::Restrain,
            Self::RealmTalk => ActivityKind::RealmTalk,
            Self::Work => ActivityKind::Work,
            Self::Gather => ActivityKind::Gather,
            Self::Load { .. } => ActivityKind::Load,
            Self::Take => ActivityKind::Take,
            Self::Construct => ActivityKind::Construct,
            Self::Combat => ActivityKind::Combat,
            Self::ArmyApproach => ActivityKind::ArmyApproach,
            Self::Play { .. } => ActivityKind::Play,
            Self::Cultivate { .. } => ActivityKind::Cultivate,
            _ => ActivityKind::Other,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DependentActivity {
    Express,
    Play,
}

pub trait SavePort {
    type Operation: Copy + PartialEq;
    fn person_exists(&self, actor: PersonId) -> bool;
    fn inventory_good(&self, inventory: InventoryId) -> Option<Good>;
    fn health_available(&self) -> bool;
    fn cultivation(&self, actor: PersonId) -> Option<(SiteId, Self::Operation, SimTime)>;
    fn conception(&self, pregnancy: PregnancyId) -> Option<([PersonId; 2], SimTime)>;
    fn validate_dependent(
        &self,
        actor: PersonId,
        activity: DependentActivity,
        completes: SimTime,
    ) -> Result<(), Error>;
    fn validate_load(
        &self,
        actor: PersonId,
        source: InventoryId,
        destination: InventoryId,
        quantity: Quantity,
        completes: SimTime,
    ) -> Result<(), Error>;
}

pub fn validate_activity_rows<P: SavePort>(
    rows: &[(PersonId, SavedActivity<P::Operation>, Location)],
    deadlines: &BTreeSet<PersonId>,
    now: SimTime,
    port: &P,
) -> Result<(), Error> {
    let fail = || Error::InvalidSave("inconsistent physical state".into());
    let mut care_targets = BTreeSet::new();
    for (actor, activity, location) in rows {
        if activity.requires_deadline() != deadlines.contains(actor) {
            return Err(fail());
        }
        validate_location_activity(location, activity.kind())?;
        match *activity {
            SavedActivity::Cultivate {
                field,
                operation,
                completes,
            } if port.cultivation(*actor) != Some((field, operation, completes)) => {
                return Err(fail());
            }
            SavedActivity::Express { completes } => {
                port.validate_dependent(*actor, DependentActivity::Express, completes)?
            }
            SavedActivity::Play { completes } => {
                port.validate_dependent(*actor, DependentActivity::Play, completes)?
            }
            SavedActivity::Unbind { target } if !port.person_exists(target) => return Err(fail()),
            SavedActivity::Care { child, .. } | SavedActivity::Comfort { child }
                if !care_targets.insert(child) =>
            {
                return Err(fail());
            }
            SavedActivity::Eat { source } if port.inventory_good(source) != Some(Good::Food) => {
                return Err(fail());
            }
            SavedActivity::Load {
                source,
                destination,
                quantity,
                completes,
            } => port.validate_load(*actor, source, destination, quantity, completes)?,
            SavedActivity::Tell {
                recipient,
                inventory,
                observed_at,
            } if !port.person_exists(recipient)
                || port.inventory_good(inventory).is_none()
                || observed_at > now =>
            {
                return Err(fail());
            }
            SavedActivity::Comfort { child } if !port.person_exists(child) => return Err(fail()),
            SavedActivity::Care { child, source }
                if !port.person_exists(child)
                    || !port.inventory_good(source).is_some_and(|good| {
                        good == Good::Food || good == Good::Water && port.health_available()
                    }) =>
            {
                return Err(fail());
            }
            SavedActivity::Conceive {
                pregnancy,
                completes,
            } if !port
                .conception(pregnancy)
                .is_some_and(|(participants, deadline)| {
                    participants.contains(actor) && deadline == completes
                }) =>
            {
                return Err(fail());
            }
            _ => {}
        }
    }
    Ok(())
}

pub fn validate_deadline<Operation>(
    activity: &SavedActivity<Operation>,
    scheduled: SimTime,
    completes: Option<SimTime>,
) -> Result<(), Error> {
    if activity.requires_deadline() && completes == Some(scheduled) {
        Ok(())
    } else {
        Err(Error::InvalidSave("inconsistent committed state".into()))
    }
}

pub fn validate_activity_population(
    actual: impl IntoIterator<Item = PersonId>,
    expected: &BTreeSet<PersonId>,
) -> Result<(), Error> {
    if actual.into_iter().collect::<BTreeSet<_>>() == *expected {
        Ok(())
    } else {
        Err(Error::InvalidSave("inconsistent physical state".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Facts;
    impl SavePort for Facts {
        type Operation = u8;
        fn person_exists(&self, actor: PersonId) -> bool {
            actor == PersonId(3)
        }
        fn inventory_good(&self, _: InventoryId) -> Option<Good> {
            Some(Good::Food)
        }
        fn health_available(&self) -> bool {
            false
        }
        fn cultivation(&self, _: PersonId) -> Option<(SiteId, Self::Operation, SimTime)> {
            None
        }
        fn conception(&self, _: PregnancyId) -> Option<([PersonId; 2], SimTime)> {
            None
        }
        fn validate_dependent(
            &self,
            _: PersonId,
            _: DependentActivity,
            _: SimTime,
        ) -> Result<(), Error> {
            Ok(())
        }
        fn validate_load(
            &self,
            _: PersonId,
            _: InventoryId,
            _: InventoryId,
            _: Quantity,
            _: SimTime,
        ) -> Result<(), Error> {
            Ok(())
        }
    }
    #[test]
    fn saved_activity_population_rejects_missing_and_foreign_actor_rows() {
        let expected = BTreeSet::from([PersonId(1), PersonId(2)]);
        assert!(validate_activity_population([PersonId(2), PersonId(1)], &expected).is_ok());
        assert!(validate_activity_population([PersonId(1)], &expected).is_err());
        assert!(
            validate_activity_population([PersonId(1), PersonId(2), PersonId(3)], &expected)
                .is_err()
        );
    }
    #[test]
    fn shifted_completion_invalidates_a_saved_deadline() {
        let eating = SavedActivity::<u8>::Eat {
            source: InventoryId(1),
        };
        assert!(validate_deadline(&eating, SimTime(10), Some(SimTime(10))).is_ok());
        assert!(validate_deadline(&eating, SimTime(10), Some(SimTime(11))).is_err());
        assert!(
            validate_deadline(&SavedActivity::<u8>::Idle, SimTime(10), Some(SimTime(10))).is_err()
        );
    }
    #[test]
    fn activity_validation_rejects_duplicate_care_and_missing_deadline() {
        let row = (
            PersonId(1),
            SavedActivity::Care {
                child: PersonId(3),
                source: InventoryId(4),
            },
            Location::AtArea(AreaId(5)),
        );
        assert!(
            validate_activity_rows(
                std::slice::from_ref(&row),
                &BTreeSet::from([PersonId(1)]),
                SimTime(0),
                &Facts
            )
            .is_ok()
        );
        assert!(
            validate_activity_rows(
                std::slice::from_ref(&row),
                &BTreeSet::new(),
                SimTime(0),
                &Facts
            )
            .is_err()
        );
        let other = (
            PersonId(2),
            SavedActivity::Comfort { child: PersonId(3) },
            Location::AtArea(AreaId(5)),
        );
        assert!(
            validate_activity_rows(
                &[row, other],
                &BTreeSet::from([PersonId(1), PersonId(2)]),
                SimTime(0),
                &Facts
            )
            .is_err()
        );
    }
    #[test]
    fn persisted_cultivation_requires_matching_foreign_job() {
        let row = (
            PersonId(1),
            SavedActivity::Cultivate {
                field: SiteId(2),
                operation: 1u8,
                completes: SimTime(10),
            },
            Location::AtArea(AreaId(5)),
        );
        assert!(
            validate_activity_rows(&[row], &BTreeSet::from([PersonId(1)]), SimTime(0), &Facts)
                .is_err()
        );
    }
}
