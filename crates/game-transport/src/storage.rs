//! Seven optional personal transport columns share the host's entity identity.
use crate::*;
use game_ecs::{Schema, StableId, Store};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OfferInbox {
    pub by_bearer: BTreeMap<PersonId, Offer>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResumeTarget {
    pub area: AreaId,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PausedApproach {
    pub target: RoadPosition,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupportKnowledge {
    pub by_passenger: BTreeMap<PersonId, SupportRequest>,
}
fn invalid() -> Error {
    Error::InvalidSave("invalid transport component".into())
}
fn road_valid(road: RoadPosition) -> bool {
    road.from.0 > 0 && road.to.0 > 0 && road.valid()
}
impl CarryGroup {
    pub fn validate(&self) -> Result<(), Error> {
        if self.passenger.0 == 0
            || self.bearers.is_empty()
            || self.bearers.len() > 16
            || !self.bearers.contains(&self.anchor)
            || self.bearers.contains(&self.passenger)
            || self.bearers.iter().any(|id| id.0 == 0)
        {
            return Err(invalid());
        }
        Ok(())
    }
}
impl Offer {
    pub fn validate(&self) -> Result<(), Error> {
        if self.passenger.0 == 0
            || self.bearer.0 == 0
            || self.passenger == self.bearer
            || self.expires <= self.offered_at
        {
            return Err(invalid());
        }
        Ok(())
    }
}
impl OfferInbox {
    pub fn validate(&self) -> Result<(), Error> {
        if self.by_bearer.is_empty() || self.by_bearer.len() > 16 {
            return Err(invalid());
        }
        let mut passenger = None;
        for (bearer, offer) in &self.by_bearer {
            offer.validate()?;
            if *bearer != offer.bearer || passenger.is_some_and(|id| id != offer.passenger) {
                return Err(invalid());
            }
            passenger = Some(offer.passenger);
        }
        Ok(())
    }
}
impl Lift {
    pub fn validate(&self) -> Result<(), Error> {
        if self.passenger.0 == 0
            || self.bearers.is_empty()
            || self.bearers.len() > 16
            || self.bearers.contains(&self.passenger)
            || self.bearers.iter().any(|id| id.0 == 0)
            || self.completes <= self.started
        {
            return Err(invalid());
        }
        Ok(())
    }
}
impl Approach {
    pub fn validate(&self) -> Result<(), Error> {
        if !road_valid(self.target) || self.completes <= self.started {
            return Err(invalid());
        }
        Ok(())
    }
}
impl ResumeTarget {
    pub fn validate(&self) -> Result<(), Error> {
        if self.area.0 == 0 {
            Err(invalid())
        } else {
            Ok(())
        }
    }
}
impl PausedApproach {
    pub fn validate(&self) -> Result<(), Error> {
        if road_valid(self.target) {
            Ok(())
        } else {
            Err(invalid())
        }
    }
}
impl SupportRequest {
    pub fn validate(&self) -> Result<(), Error> {
        if self.passenger.0 == 0 || self.requester.0 == 0 || self.expires <= self.observed_at {
            return Err(invalid());
        }
        Ok(())
    }
}
impl SupportKnowledge {
    pub fn validate(&self) -> Result<(), Error> {
        if self.by_passenger.len() > 64 {
            return Err(invalid());
        }
        for (id, request) in &self.by_passenger {
            request.validate()?;
            if *id != request.passenger {
                return Err(invalid());
            }
        }
        Ok(())
    }
}
impl Response {
    pub fn validate(&self) -> Result<(), Error> {
        if self.passenger.0 == 0
            || self.bearer.0 == 0
            || self.reason.is_empty()
            || self.reason.len() > 256
        {
            return Err(invalid());
        }
        Ok(())
    }
}
pub fn register_components(schema: &mut Schema) -> Result<(), Error> {
    schema.register::<CarryGroup>("transport/group", CarryGroup::validate)?;
    schema.register::<OfferInbox>("transport/offers", OfferInbox::validate)?;
    schema.register::<Lift>("transport/lift", Lift::validate)?;
    schema.register::<Approach>("transport/approach", Approach::validate)?;
    schema.register::<ResumeTarget>("transport/resume", ResumeTarget::validate)?;
    schema.register::<PausedApproach>("transport/paused", PausedApproach::validate)?;
    schema.register::<SupportKnowledge>("transport/support", SupportKnowledge::validate)
}
macro_rules! ports {
    ($row:ty,$get:ident,$mut:ident,$iter:ident,$insert:ident,$remove:ident,$identity:expr) => {
        pub fn $get(store: &Store, id: PersonId) -> Option<&$row> {
            store.get(StableId(id.0))
        }
        pub fn $mut(store: &mut Store, id: PersonId) -> Option<&mut $row> {
            store.get_mut(StableId(id.0))
        }
        pub fn $iter(store: &Store) -> impl Iterator<Item = (PersonId, &$row)> {
            store.iter::<$row>().map(|(id, row)| (PersonId(id.0), row))
        }
        pub fn $insert(store: &mut Store, id: PersonId, row: $row) -> Result<(), Error> {
            row.validate()?;
            if !($identity)(id, &row) {
                return Err(invalid());
            }
            store.insert_or_spawn(StableId(id.0), row)
        }
        pub fn $remove(store: &mut Store, id: PersonId) -> Result<Option<$row>, Error> {
            if !store.contains(StableId(id.0)) {
                return Ok(None);
            }
            store.remove(StableId(id.0))
        }
    };
}
ports!(
    CarryGroup,
    group,
    group_mut,
    groups,
    insert_group,
    remove_group,
    |id: PersonId, row: &CarryGroup| id == row.passenger
);
ports!(
    OfferInbox,
    offer_inbox,
    offer_inbox_mut,
    offer_inboxes,
    insert_offer_inbox,
    remove_offer_inbox,
    |id: PersonId, row: &OfferInbox| row.by_bearer.values().all(|offer| offer.passenger == id)
);
ports!(
    Lift,
    lift,
    lift_mut,
    lifts,
    insert_lift,
    remove_lift,
    |id: PersonId, row: &Lift| id == row.passenger
);
ports!(
    Approach,
    approach_job,
    approach_job_mut,
    approaches,
    insert_approach,
    remove_approach,
    |_: PersonId, _: &Approach| true
);
ports!(
    ResumeTarget,
    resume_target,
    resume_target_mut,
    resume_targets,
    insert_resume_target,
    remove_resume_target,
    |_: PersonId, _: &ResumeTarget| true
);
ports!(
    PausedApproach,
    paused_approach,
    paused_approach_mut,
    paused_approaches,
    insert_paused_approach,
    remove_paused_approach,
    |_: PersonId, _: &PausedApproach| true
);
ports!(
    SupportKnowledge,
    support_knowledge,
    support_knowledge_mut,
    support_knowledges,
    insert_support_knowledge,
    remove_support_knowledge,
    |_: PersonId, _: &SupportKnowledge| true
);
