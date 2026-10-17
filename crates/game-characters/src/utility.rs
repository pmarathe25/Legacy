use super::*;
#[derive(Clone, Serialize, Deserialize)]
pub struct IdleReview {
    pub person: PersonId,
}
impl Event for IdleReview {
    const KEY: &'static str = "characters.idle-review";
}
