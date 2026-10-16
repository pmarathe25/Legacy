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
impl Relationship {
    pub fn validate(&self) -> Result<()> {
        require(
            (-100..=100).contains(&self.trust)
                && (-100..=100).contains(&self.affection)
                && (0..=100).contains(&self.fear)
                && (0..=100).contains(&self.grievance),
            "invalid relationship",
        )
    }
}
/// Authored starting attachments; subsequent changes use experience requests.
#[derive(Clone, Serialize, Deserialize)]
pub struct Initialize {
    pub person: PersonId,
    pub beliefs: Beliefs,
    pub values: std::collections::BTreeMap<String, i32>,
}
impl Request for Initialize {
    const KEY: &'static str = "relationships.initialize";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct SetRelationship {
    pub observer: PersonId,
    pub other: PersonId,
    pub relationship: Relationship,
}
impl Request for SetRelationship {
    const KEY: &'static str = "relationships.set";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Converse {
    pub observer: PersonId,
    pub other: PersonId,
    pub warmth: i32,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct SufferForce {
    pub observer: PersonId,
    pub actor: PersonId,
    pub harm: u64,
}
impl Request for SufferForce {
    const KEY: &'static str = "relationships.suffer-force";
    type Response = ();
}
impl Request for Converse {
    const KEY: &'static str = "relationships.converse";
    type Response = ();
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Social {
    pub relationships: Table<PersonId, Table<PersonId, Relationship>>,
    pub beliefs: Table<PersonId, Beliefs>,
    pub values: Table<PersonId, Table<Arc<str>, i32>>,
}
impl Resource for Social {
    const KEY: &'static str = "relationships.social";
    fn validate(&self) -> Result<()> {
        for (person, links) in &self.relationships {
            for (other, relation) in links {
                require(person != other, "self relationship")?;
                relation.validate()?;
            }
        }
        for values in self.values.values() {
            validate_values(values)?;
        }
        Ok(())
    }
    fn validate_changes(&self, old: &Self) -> Result<()> {
        for (person, links) in self.relationships.changed(&old.relationships) {
            for (other, relation) in links {
                require(person != other, "self relationship")?;
                relation.validate()?;
            }
        }
        for (_, values) in self.values.changed(&old.values) {
            validate_values(values)?;
        }
        Ok(())
    }
}
fn validate_values(values: &Table<Arc<str>, i32>) -> Result<()> {
    require(
        values
            .iter()
            .all(|(key, v)| !key.trim().is_empty() && (-100..=100).contains(v)),
        "invalid personal values",
    )
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
#[derive(Clone, Serialize, Deserialize)]
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
#[derive(Clone, Serialize, Deserialize)]
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
    b.request::<SufferForce>(|tx, r| {
        require(
            r.observer != r.actor && r.harm > 0,
            "invalid force experience",
        )?;
        let rel = tx
            .edit::<Social>()?
            .relationships
            .entry(r.observer)
            .or_default()
            .entry(r.actor)
            .or_default();
        let harm = r.harm.min(100) as i32;
        rel.fear = (rel.fear + harm).min(100);
        rel.trust = (rel.trust - harm).max(-100);
        rel.grievance = (rel.grievance + harm / 2).min(100);
        Ok(())
    })?;
    b.request::<Initialize>(|tx, r| {
        require(
            !tx.get::<Social>()?.beliefs.contains_key(&r.person),
            "social profile already initialized",
        )?;
        require(
            r.values
                .iter()
                .all(|(k, v)| !k.trim().is_empty() && (-100..=100).contains(v)),
            "invalid starting values",
        )?;
        let state = tx.edit::<Social>()?;
        state.beliefs.insert(r.person, r.beliefs);
        state.values.insert(
            r.person,
            r.values.into_iter().map(|(k, v)| (k.into(), v)).collect(),
        );
        Ok(())
    })?;
    b.request::<SetRelationship>(|tx, r| {
        require(r.observer != r.other, "self relationship")?;
        r.relationship.validate()?;
        tx.edit::<Social>()?
            .relationships
            .entry(r.observer)
            .or_default()
            .insert(r.other, r.relationship);
        Ok(())
    })?;
    b.request::<Converse>(|tx, r| {
        require(r.observer != r.other, "self conversation")?;
        let rel = tx
            .edit::<Social>()?
            .relationships
            .entry(r.observer)
            .or_default()
            .entry(r.other)
            .or_default();
        let warmth = r.warmth.clamp(-5, 5);
        rel.affection = rel.affection.saturating_add(warmth).clamp(-100, 100);
        rel.trust = rel.trust.saturating_add(warmth.signum()).clamp(-100, 100);
        if warmth < 0 {
            rel.grievance = (rel.grievance + 1).min(100);
        }
        Ok(())
    })?;
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
