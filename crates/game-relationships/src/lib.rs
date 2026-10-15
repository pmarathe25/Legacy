//! Directed relationships, distinct beliefs, and learned values.
use game_core::*;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Relationship {
    pub trust: i32,
    pub affection: i32,
    pub fear: i32,
    pub grievance: i32,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Beliefs {
    pub sharing: i32,
    pub authority: i32,
    pub honesty: i32,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Social {
    pub relationships: Table<PersonId, Table<PersonId, Relationship>>,
    pub beliefs: Table<PersonId, Beliefs>,
    pub values: Table<PersonId, Table<Arc<str>, i32>>,
}
impl Resource for Social {
    const KEY: &'static str = "relationships.social";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Interpret {
    pub observer: PersonId,
    pub actor: PersonId,
    pub shared: bool,
    pub harmed: bool,
    pub credible: bool,
}
impl Request for Interpret {
    const KEY: &'static str = "relationships.interpret";
    type Response = Relationship;
}
#[derive(Serialize, Deserialize)]
pub struct LearnValue {
    pub person: PersonId,
    pub concept: String,
    pub outcome: i32,
}
impl Request for LearnValue {
    const KEY: &'static str = "relationships.learn_value";
    type Response = i32;
}
pub fn reaction(beliefs: &Beliefs, event: &Interpret) -> i32 {
    if !event.credible {
        0
    } else if event.harmed {
        -20
    } else if event.shared {
        beliefs.sharing.clamp(-10, 10)
    } else {
        0
    }
}
#[derive(Serialize, Deserialize)]
pub struct DefineBeliefs {
    pub person: PersonId,
    pub beliefs: Beliefs,
}
impl Request for DefineBeliefs {
    const KEY: &'static str = "relationships.define";
    type Response = ();
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("relationships", 1)?;
    b.resource::<Social>()?;
    b.request::<DefineBeliefs>(|tx, r| {
        tx.edit::<Social>()?.beliefs.insert(r.person, r.beliefs);
        Ok(())
    })?;
    b.request::<Interpret>(|tx, r| {
        let beliefs = tx
            .get::<Social>()?
            .beliefs
            .get(&r.observer)
            .cloned()
            .unwrap_or_default();
        let delta = reaction(&beliefs, &r);
        let rel = tx
            .edit::<Social>()?
            .relationships
            .entry(r.observer)
            .or_default()
            .entry(r.actor)
            .or_default();
        rel.trust = rel.trust.saturating_add(delta).clamp(-100, 100);
        if r.credible && r.harmed {
            rel.grievance = rel.grievance.saturating_add(10).min(100);
        }
        Ok(rel.clone())
    })?;
    b.request::<LearnValue>(|tx, r| {
        require(!r.concept.is_empty(), "value needs a concept")?;
        let value = tx
            .edit::<Social>()?
            .values
            .entry(r.person)
            .or_default()
            .entry(r.concept.into())
            .or_default();
        *value = value
            .saturating_add(r.outcome.clamp(-5, 5))
            .clamp(-100, 100);
        Ok(*value)
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn same_sharing_has_observer_dependent_meaning() {
        let e = Interpret {
            observer: PersonId(1),
            actor: PersonId(2),
            shared: true,
            harmed: false,
            credible: true,
        };
        assert_eq!(
            reaction(
                &Beliefs {
                    sharing: 10,
                    ..Default::default()
                },
                &e
            ),
            10
        );
        assert_eq!(
            reaction(
                &Beliefs {
                    sharing: -10,
                    ..Default::default()
                },
                &e
            ),
            -10
        );
    }
}
