use super::*;
use game_sim::Event;
#[derive(Clone, Serialize, Deserialize)]
pub struct Died {
    pub person: PersonId,
    pub at: SimTime,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<Body>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cause: Option<DeathCause>,
}
impl Event for Died {
    const KEY: &'static str = "physiology.died";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Damaged {
    pub person: PersonId,
    pub amount: u64,
}
impl Event for Damaged {
    const KEY: &'static str = "physiology.damaged";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Rested {
    pub person: PersonId,
}
impl Event for Rested {
    const KEY: &'static str = "physiology.rested";
}
