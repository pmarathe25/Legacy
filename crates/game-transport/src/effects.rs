//! Typed receiving-owner operations derived from prepared physical motion.
use crate::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotionActivity {
    Idle,
    Lift {
        passenger: PersonId,
        completes: SimTime,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhysicalPlacement {
    pub actor: PersonId,
    pub area: Option<AreaId>,
}
pub struct PreparedMotionReactions {
    pub interruptions: Vec<PersonId>,
    pub activities: BTreeMap<PersonId, MotionActivity>,
    pub shelter_departures: Vec<PersonId>,
    pub placements: Vec<PhysicalPlacement>,
    lift_reactions: PreparedLiftReactions,
    movement_events: u64,
}
impl PreparedMotionReactions {
    pub fn validate(&self, facts: &impl LiftActionFacts) -> Result<(), Error> {
        self.lift_reactions.validate(facts)
    }
    pub fn cancelled_deadlines(&self, passenger: PersonId) -> impl Iterator<Item = PersonId> + '_ {
        self.lift_reactions.resets_for(passenger)
    }
    pub fn movement_events(&self) -> u64 {
        self.movement_events
    }
}
pub fn prepare_motion_reactions(
    effects: &[MotionEffect],
    facts: &impl LiftActionFacts,
) -> Result<PreparedMotionReactions, Error> {
    let cancelled = effects
        .iter()
        .filter_map(|effect| match effect {
            MotionEffect::LiftCancelled(lift) => Some(lift.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let lift_reactions = prepare_lift_reactions(&cancelled, facts);
    let mut activities = lift_reactions
        .resets()
        .map(|actor| (actor, MotionActivity::Idle))
        .collect::<BTreeMap<_, _>>();
    let mut interruptions = Vec::new();
    let mut shelter_departures = Vec::new();
    let mut placements = Vec::new();
    let mut movement_events = 0_u64;
    for effect in effects {
        match *effect {
            MotionEffect::StartLift {
                actor,
                passenger,
                completes,
            } => {
                activities.insert(
                    actor,
                    MotionActivity::Lift {
                        passenger,
                        completes,
                    },
                );
            }
            MotionEffect::Interrupt(actor) => interruptions.push(actor),
            MotionEffect::LeaveShelter(actor) => shelter_departures.push(actor),
            MotionEffect::PutDown { actor, area } => {
                placements.push(PhysicalPlacement { actor, area })
            }
            MotionEffect::Departed { .. }
            | MotionEffect::Interrupted { .. }
            | MotionEffect::Arrived { .. } => {
                movement_events = movement_events.checked_add(1).ok_or(Error::Overflow)?;
            }
            _ => {}
        }
    }
    Ok(PreparedMotionReactions {
        interruptions,
        activities,
        shelter_departures,
        placements,
        lift_reactions,
        movement_events,
    })
}
