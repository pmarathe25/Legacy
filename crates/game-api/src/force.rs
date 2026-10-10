//! Visible confinement and concrete attempts to leave it.
use crate::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfinementView {
    pub target: PersonId,
    pub captor: PersonId,
    pub location: Location,
    pub captured_position: Point,
    pub can_escape: bool,
    pub escaping: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForceView {
    pub confinements: Vec<ConfinementView>,
}
