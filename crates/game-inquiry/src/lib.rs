//! Personally witnessed episodes, imperfect explanations, deduction and voluntary teaching.
use game_core::*;
use game_reasoning::{Claim, Observation};
use game_sim::*;
use game_storage::Records;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
mod choices;
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Experience {
    pub observations: VecDeque<std::sync::Arc<Observation>>,
    pub count: u64,
    pub considered: u64,
    #[serde(default)]
    pub reasoning_signature: u64,
    #[serde(default)]
    pub capacity: u64,
    #[serde(default)]
    pub contrasts: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Inquiry {
    pub people: Records<PersonId, Experience>,
}
impl Resource for Inquiry {
    const KEY: &'static str = "inquiry.experience";
    fn validate(&self) -> Result<()> {
        validate_memories(self.people.values())
    }
    fn validate_changes(&self, old: &Self) -> Result<()> {
        validate_memories(self.people.changed(&old.people).map(|(_, memory)| memory))
    }
}
fn validate_memories<'a>(mut memories: impl Iterator<Item = &'a Experience>) -> Result<()> {
    require(
        memories.all(|p| p.observations.len() <= 128 && p.considered <= p.count),
        "invalid empirical memory",
    )
}
/// Feature owners supply personally observable episodes, never hidden rules or probabilities.
#[derive(Clone, Serialize, Deserialize)]
pub struct Witness {
    pub observer: PersonId,
    pub observation: Observation,
}
impl Request for Witness {
    const KEY: &'static str = "inquiry.witness";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Reflect {
    pub person: PersonId,
}
impl Request for Reflect {
    const KEY: &'static str = "inquiry.reflect";
    type Response = usize;
}
#[derive(Clone, Copy, Serialize, Deserialize)]
pub enum Explanation {
    Detailed,
    Practical,
    Demonstrate,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Explain {
    pub teacher: PersonId,
    pub listener: PersonId,
    pub key: String,
    pub form: Explanation,
}
impl Request for Explain {
    const KEY: &'static str = "inquiry.explain";
    type Response = bool;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Learned {
    pub person: PersonId,
    pub key: String,
    pub confidence: u8,
    pub deduction: bool,
}
impl Event for Learned {
    const KEY: &'static str = "inquiry.learned";
}
/// Contrasting personal episodes have no remembered explanation yet.
#[derive(Clone, Serialize, Deserialize)]
pub struct Uncertain {
    pub person: PersonId,
    pub subject: String,
}
impl Event for Uncertain {
    const KEY: &'static str = "inquiry.uncertain";
}
fn perceived_claims<'a>(
    tx: &'a Transaction<'_>,
    p: PersonId,
) -> impl Iterator<Item = (std::borrow::Cow<'a, Claim>, u8)> {
    tx.get::<game_knowledge::Knowledge>()
        .ok()
        .and_then(|k| k.0.get(&p))
        .into_iter()
        .flat_map(|f| game_knowledge::with_prefix(f, "theory:"))
        .filter_map(|(_, f)| {
            let claim = match f.value.as_ref() {
                game_knowledge::FactValue::Theory(c) => std::borrow::Cow::Borrowed(c.as_ref()),
                _ => std::borrow::Cow::Owned(f.value.decode::<Claim>()?),
            };
            Some((claim, f.confidence))
        })
        .take(128)
}
fn claims(tx: &Transaction<'_>, p: PersonId) -> Vec<Claim> {
    perceived_claims(tx, p)
        .map(|(claim, confidence)| {
            let mut claim = claim.into_owned();
            claim.confidence = claim.confidence.min(confidence);
            claim
        })
        .collect()
}
fn signature(c: &Claim, confidence: u8) -> u64 {
    use std::fmt::Write;
    let mut hash = game_core::Fingerprint::default();
    c.write_key(&mut hash).unwrap();
    write!(
        hash,
        ":{}:{}:{}",
        c.effect,
        c.confidence.min(confidence),
        c.complexity
    )
    .unwrap();
    hash.finish()
}
fn evidence(tx: &Transaction<'_>, p: PersonId) -> (usize, u64) {
    perceived_claims(tx, p).fold((0, 0), |(count, hash), (c, confidence)| {
        (count + 1, hash ^ signature(&c, confidence))
    })
}
fn accepts(tx: &Transaction<'_>, p: PersonId, claim: &Claim) -> bool {
    let Some(old) = tx
        .get::<game_knowledge::Knowledge>()
        .ok()
        .and_then(|k| k.0.get(&p))
        .and_then(|f| game_knowledge::known::<Claim>(f, &claim.key()))
    else {
        return true;
    };
    if old.effect.signum() == claim.effect.signum() {
        return true;
    }
    let rigidity = game_vitality::disposition(tx, p, "rigidity");
    i64::from(claim.confidence) * (1000 - rigidity) > i64::from(old.confidence) * rigidity
}
fn retain(tx: &mut Transaction<'_>, p: PersonId, c: Claim, deduction: bool) -> Result<()> {
    if !accepts(tx, p, &c) {
        return Ok(());
    }
    let key = c.key();
    let confidence = c.confidence;
    let previous = tx
        .get::<game_knowledge::Knowledge>()?
        .0
        .get(&p)
        .and_then(|f| game_knowledge::known::<Claim>(f, &key));
    if previous.as_ref() == Some(&c)
        || deduction
            && previous.as_ref().is_some_and(|old| {
                old.effect == c.effect
                    && old.complexity <= c.complexity
                    && old.confidence >= c.confidence
            })
    {
        return Ok(());
    }
    tx.call(game_knowledge::Conclude {
        person: p,
        key: key.clone(),
        value: serde_json::to_value(c).unwrap(),
        confidence,
    })?;
    tx.emit(
        Learned {
            person: p,
            key,
            confidence,
            deduction,
        },
        vec![p],
    )
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("inquiry", 1)?;
    b.resource::<Inquiry>()?;
    b.request::<game_testimony::Comprehend>(|tx, r| {
        let Some(claim) = r.fact.value.decode::<Claim>() else {
            return Ok(true);
        };
        if !accepts(tx, r.person, &claim) {
            return Ok(false);
        }
        let ability = game_vitality::ability(tx, r.person, "intelligence");
        Ok(ability.saturating_mul(1000 + r.support.min(16000)) / 1000
            >= u64::from(claim.complexity) * 250)
    })?;
    choices::register(b)?;
    b.request::<Witness>(|tx, r| {
        require(
            !r.observation.subject.is_empty()
                && r.observation.subject.len() <= 48
                && !r.observation.outcome.is_empty()
                && r.observation.outcome.len() <= 24
                && r.observation.context.len() <= 16
                && r.observation
                    .context
                    .keys()
                    .all(|k| !k.is_empty() && k.len() <= 24),
            "invalid perceived episode",
        )?;
        tx.call(game_knowledge::Conclude {
            person: r.observer,
            key: format!(
                "measured:{}:{}",
                r.observation.subject, r.observation.outcome
            ),
            value: serde_json::to_value(&r.observation).unwrap(),
            confidence: 80,
        })?;
        let memory = (8 + game_vitality::ability(tx, r.observer, "intelligence").saturating_mul(
            500 + game_vitality::ability(tx, r.observer, "attention").min(8000) / 2,
        ) / 125000)
            .clamp(8, 128) as usize;
        let m = tx.edit::<Inquiry>()?.people.entry(r.observer).or_default();
        let contrasted = m.observations.iter().rev().take(16).any(|old| {
            old.subject == r.observation.subject
                && old.outcome == r.observation.outcome
                && old.context != r.observation.context
        });
        m.contrasts |= contrasted;
        let subject = r.observation.subject.clone();
        m.observations.push_back(std::sync::Arc::new(r.observation));
        m.count = m.count.saturating_add(1);
        while m.observations.len() > memory {
            m.observations.pop_front();
        }
        let explained = tx
            .get::<game_knowledge::Knowledge>()?
            .0
            .get(&r.observer)
            .is_some_and(|facts| {
                game_knowledge::with_prefix(facts, &format!("theory:{subject}:"))
                    .next()
                    .is_some()
            });
        if contrasted && !explained {
            tx.notify(Uncertain {
                person: r.observer,
                subject,
            })?;
        }
        Ok(())
    })?;
    b.request::<Reflect>(|tx, r| {
        let intelligence = game_vitality::ability(tx, r.person, "intelligence").min(16000);
        let observations: Vec<_> = tx
            .get::<Inquiry>()?
            .people
            .get(&r.person)
            .into_iter()
            .flat_map(|p| p.observations.iter())
            .map(|o| o.as_ref().clone())
            .collect();
        let inferred = game_reasoning::infer(
            &observations,
            if intelligence >= 500 {
                (8 + intelligence / 2) as usize
            } else {
                0
            },
            (intelligence / 500).saturating_sub(1).min(15) as usize,
        );
        let n = inferred.len();
        for c in inferred {
            retain(tx, r.person, c, false)?;
        }
        let known = claims(tx, r.person);
        for c in game_reasoning::deduce(
            &known,
            (intelligence / 2) as usize,
            (intelligence / 250).min(64) as u16,
        ) {
            retain(tx, r.person, c, true)?;
        }
        let (_, signature) = evidence(tx, r.person);
        let m = tx.edit::<Inquiry>()?.people.entry(r.person).or_default();
        m.considered = m.count;
        m.contrasts = false;
        m.capacity = intelligence / 250;
        m.reasoning_signature = signature;
        Ok(n)
    })?;
    b.request::<Explain>(|tx, r| {
        game_physiology::capable(tx, r.teacher)?;
        game_physiology::capable(tx, r.listener)?;
        let speech = game_vitality::ability(tx, r.teacher, "vocalization") > 0
            && game_vitality::ability(tx, r.listener, "hearing") > 0;
        let demonstration = matches!(r.form, Explanation::Demonstrate)
            && game_vitality::ability(tx, r.teacher, "gesturing") > 0
            && game_vitality::ability(tx, r.listener, "interpretation") > 0;
        require(speech || demonstration, "no usable explanatory channel")?;
        require(
            game_transport::area(tx.get()?, r.teacher)?
                == game_transport::area(tx.get()?, r.listener)?,
            "explanation needs contact",
        )?;
        let claim = tx
            .get::<game_knowledge::Knowledge>()?
            .0
            .get(&r.teacher)
            .and_then(|f| game_knowledge::known::<Claim>(f, &r.key))
            .ok_or_else(|| Error::Rejected("teacher does not know this explanation".into()))?;
        let ability = game_vitality::ability(tx, r.listener, "intelligence");
        let teaching = game_vitality::ability(tx, r.teacher, "intelligence") / 10
            + game_vitality::disposition(tx, r.teacher, "patience").max(0) as u64;
        let difficulty = u64::from(claim.complexity) * 250;
        let support = (teaching
            + match r.form {
                Explanation::Detailed => 0,
                Explanation::Practical => 3000,
                Explanation::Demonstrate => 6000,
            })
        .min(16000);
        let regard = tx
            .get::<game_relationships::Social>()
            .ok()
            .and_then(|s| s.relationships.get(&r.listener)?.get(&r.teacher))
            .map_or(0, |r| {
                i64::from(r.trust + r.respect + r.admiration - r.grievance)
            });
        if ability.saturating_mul(1000 + support) / 1000 < difficulty
            || game_vitality::disposition(tx, r.listener, "openness") / 10 + regard < 20
        {
            return Ok(false);
        }
        // Simplification changes the cognitive demand, not the conditions or certainty of the claim.
        tx.call(game_testimony::Explain {
            sender: r.teacher,
            recipient: r.listener,
            key: r.key,
            demonstrate: matches!(r.form, Explanation::Demonstrate),
            support,
        })?;
        Ok(true)
    })
}

#[cfg(test)]
mod signature_tests {
    use super::*;
    #[test]
    fn streamed_signatures_preserve_reasoning_progress_and_confidence_sensitivity() {
        for effect in [i64::MIN, -500, 0, 20, i64::MAX] {
            let claim = Claim {
                subject: "food:storage".into(),
                factor: "warmth".into(),
                outcome: "duration".into(),
                effect,
                complexity: 4,
                confidence: 80,
                comparisons: 15,
                context: [("café".into(), -200)].into(),
            };
            for confidence in [0, 1, 40, 80, 100, 255] {
                let old = game_choice::identity(&format!(
                    "{}:{}:{}:{}",
                    claim.key(),
                    claim.effect,
                    claim.confidence.min(confidence),
                    claim.complexity
                ));
                assert_eq!(signature(&claim, confidence), old);
            }
        }
    }
}
