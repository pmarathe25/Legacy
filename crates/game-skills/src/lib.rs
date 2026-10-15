//! Learning attribution prevents crediting one interval twice.
use game_core::*;
use game_sim::*;
use game_storage::{Records, Table};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Skills {
    pub levels: Records<PersonId, BTreeMap<String, u64>>,
    pub credited: Table<u64, ()>,
}
impl Resource for Skills {
    const KEY: &'static str = "skills.practice";
}
#[derive(Serialize, Deserialize)]
pub struct Practice {
    pub person: PersonId,
    pub skill: String,
    pub effort: u64,
    pub attribution: u64,
}
impl Request for Practice {
    const KEY: &'static str = "skills.practice";
    type Response = u64;
}
pub fn gain(level: u64, effort: u64) -> u64 {
    effort / (1 + level / 100)
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("skills", 1)?;
    b.resource::<Skills>()?;
    b.request::<Practice>(|tx, r| {
        require(!r.skill.is_empty() && r.effort > 0, "invalid practice")?;
        let s = tx.edit::<Skills>()?;
        require(
            s.credited.insert(r.attribution, ()).is_none(),
            "work interval already credited",
        )?;
        let level = s
            .levels
            .entry(r.person)
            .or_default()
            .entry(r.skill)
            .or_default();
        *level = level
            .checked_add(gain(*level, r.effort))
            .ok_or(Error::Overflow)?;
        Ok(*level)
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn practice_has_diminishing_returns_and_cannot_double_count() {
        let mut b = Builder::new();
        plugin(&mut b).unwrap();
        let mut h = b.build(1, 1).unwrap();
        assert_eq!(
            h.dispatch(Practice {
                person: PersonId(1),
                skill: "farm".into(),
                effort: 100,
                attribution: 2
            })
            .unwrap(),
            100
        );
        assert!(
            h.dispatch(Practice {
                person: PersonId(1),
                skill: "farm".into(),
                effort: 100,
                attribution: 2
            })
            .is_err()
        );
        assert_eq!(
            h.dispatch(Practice {
                person: PersonId(1),
                skill: "farm".into(),
                effort: 100,
                attribution: 3
            })
            .unwrap(),
            150
        );
    }
}
