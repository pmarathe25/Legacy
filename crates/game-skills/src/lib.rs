//! Learning attribution prevents crediting one interval twice.
use game_core::*;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
pub type Levels = Table<Arc<str>, u64>;
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Skills {
    pub levels: Table<PersonId, Levels>,
    pub credited: Table<u64, ()>,
}
impl Resource for Skills {
    const KEY: &'static str = "skills.practice";
}
#[derive(Clone, Serialize, Deserialize)]
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
            .entry(r.skill.into())
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

#[cfg(test)]
mod history_tests {
    use super::*;
    #[test]
    fn extending_a_large_profile_preserves_older_snapshots_and_attribution() {
        let mut b = Builder::new();
        plugin(&mut b).unwrap();
        let mut host = b.build(1, 1).unwrap();
        for i in 0..1000 {
            host.dispatch(Practice {
                person: PersonId(1),
                skill: format!("skill:{i}"),
                effort: 10,
                attribution: i + 1,
            })
            .unwrap();
        }
        let before = host.get::<Skills>().unwrap().clone();
        host.dispatch(Practice {
            person: PersonId(1),
            skill: "skill:500".into(),
            effort: 10,
            attribution: 1001,
        })
        .unwrap();
        assert_eq!(before.levels[&PersonId(1)]["skill:500"], 10);
        assert_eq!(
            host.get::<Skills>().unwrap().levels[&PersonId(1)]["skill:500"],
            20
        );
        let save = host.save().unwrap();
        assert!(
            host.dispatch(Practice {
                person: PersonId(1),
                skill: "new-skill".into(),
                effort: 10,
                attribution: 1001
            })
            .is_err()
        );
        assert_eq!(host.save().unwrap(), save);
    }
}
