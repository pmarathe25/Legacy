//! A personal material aspiration, supplied to other owners as a factual goal.
use crate::decisions::Aspiration;
use game_core::Good;
pub fn independence_material_goal(aspiration: Aspiration) -> Option<Good> {
    (aspiration == Aspiration::Independence).then_some(Good::Tool)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn current_independence_motive_requests_a_tool_without_defining_project_content() {
        assert_eq!(
            independence_material_goal(Aspiration::Independence),
            Some(Good::Tool)
        );
        for aspiration in [
            Aspiration::Comfort,
            Aspiration::SupportOthers,
            Aspiration::Parenthood,
        ] {
            assert_eq!(independence_material_goal(aspiration), None);
        }
    }
}
