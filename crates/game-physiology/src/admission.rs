//! Personal physical admission from command kinds and borrowed owner facts.
use crate::{Body, Rules, physically_capable};
use game_core::{AreaId, Error, Location, SimTime};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PersonalActionKind {
    Signal,
    Play,
    Rest,
    Cancel,
    Release,
    Comfort,
    Care,
    Tell,
    Eat,
    Load,
    Work,
    Other,
}
pub struct PersonalActionFacts<'a> {
    pub conflict_allowed: bool,
    pub independent: bool,
    pub receiving_care: bool,
    pub body: Option<&'a Body>,
}
/// The location binding runs only after agency and capability admission.
pub fn prepare_personal_action_area(
    kind: PersonalActionKind,
    facts: PersonalActionFacts<'_>,
    location: impl FnOnce() -> Location,
) -> Result<AreaId, Error> {
    if !facts.conflict_allowed {
        return Err(Error::Unavailable);
    }
    let dependent_signal = matches!(
        kind,
        PersonalActionKind::Signal | PersonalActionKind::Cancel
    );
    if (!facts.independent || facts.receiving_care) && !dependent_signal {
        return Err(Error::Unavailable);
    }
    if !facts.body.is_some_and(physically_capable) {
        return Err(Error::Unavailable);
    }
    match location() {
        Location::AtArea(area) => Ok(area),
        Location::StoppedOnRoad { from, .. }
            if matches!(
                kind,
                PersonalActionKind::Cancel
                    | PersonalActionKind::Rest
                    | PersonalActionKind::Eat
                    | PersonalActionKind::Load
                    | PersonalActionKind::Care
                    | PersonalActionKind::Comfort
                    | PersonalActionKind::Release
                    | PersonalActionKind::Signal
            ) =>
        {
            Ok(from)
        }
        Location::InTransit { from, .. }
            if matches!(
                kind,
                PersonalActionKind::Signal | PersonalActionKind::Cancel
            ) =>
        {
            Ok(from)
        }
        _ => Err(Error::Unavailable),
    }
}
impl Rules {
    pub fn unbind_deadline(&self, now: SimTime) -> Result<SimTime, Error> {
        Ok(SimTime(
            now.0
                .checked_add(self.loading_time.0)
                .ok_or(Error::Overflow)?,
        ))
    }
}
