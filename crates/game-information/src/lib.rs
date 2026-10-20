//! Questions express personal interests; neither asking nor answering queries hidden reality.
mod choices;
mod topics;
use game_core::*;
use game_knowledge::Knowledge;
use game_sim::*;
use game_storage::Records;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub use topics::Topic;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Interest {
    pub topic: Topic,
    pub importance: i64,
    pub asked: BTreeSet<PersonId>,
    pub read: BTreeMap<u64, u64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Question {
    pub id: u64,
    pub asker: PersonId,
    pub topic: Topic,
    pub at: SimTime,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Information {
    pub interests: Records<PersonId, Vec<Interest>>,
    pub inboxes: Records<PersonId, Vec<Question>>,
}
impl Resource for Information {
    const KEY: &'static str = "information.questions";
    fn allocated_ids(&self) -> Vec<u64> {
        self.inboxes.values().flatten().map(|q| q.id).collect()
    }
    fn validate(&self) -> Result<()> {
        require(
            self.interests.values().all(|row| {
                row.len() <= 8
                    && row.iter().all(|i| {
                        i.topic.valid()
                            && i.asked.len() <= 64
                            && i.read.len() <= 64
                            && (1..=10000).contains(&i.importance)
                    })
            }) && self
                .inboxes
                .values()
                .all(|row| row.len() <= 16 && row.iter().all(|q| q.topic.valid())),
            "invalid information requests",
        )
    }
}
/// Feature owners register a need; its pursuit competes with all other choices.
#[derive(Clone, Serialize, Deserialize)]
pub struct Want {
    pub person: PersonId,
    pub topic: Topic,
    pub importance: i64,
}
impl Request for Want {
    const KEY: &'static str = "information.want";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Withdraw {
    pub person: PersonId,
    pub topic: Topic,
}
impl Request for Withdraw {
    const KEY: &'static str = "information.withdraw";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Ask {
    pub person: PersonId,
    pub other: PersonId,
    pub topic: Topic,
}
impl Request for Ask {
    const KEY: &'static str = "information.ask";
    type Response = u64;
}
/// None deliberately declines; a key selects a fact from the respondent's own memory.
#[derive(Clone, Serialize, Deserialize)]
pub struct Answer {
    pub person: PersonId,
    pub question: u64,
    pub key: Option<String>,
}
impl Request for Answer {
    const KEY: &'static str = "information.answer";
    type Response = game_choice::Outcome;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Asked {
    pub person: PersonId,
    pub other: PersonId,
    pub topic: Topic,
}
impl Event for Asked {
    const KEY: &'static str = "information.asked";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Answered {
    pub person: PersonId,
    pub asker: PersonId,
    pub topic: Topic,
    pub shared: bool,
}
impl Event for Answered {
    const KEY: &'static str = "information.answered";
}
fn contact(tx: &Transaction<'_>, a: PersonId, b: PersonId) -> Result<()> {
    require(
        a != b && game_transport::together(tx.get()?, a, b),
        "question needs contact",
    )?;
    game_physiology::capable(tx, a)?;
    game_physiology::capable(tx, b)?;
    require(
        game_vitality::can_converse(tx, a) && game_vitality::can_converse(tx, b),
        "no conversational capability",
    )
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("information", 1)?;
    b.resource::<Information>()?;
    choices::register(b)?;
    b.request::<Want>(|tx, r| {
        require(
            r.topic.valid() && (1..=10000).contains(&r.importance),
            "invalid information need",
        )?;
        let row = tx
            .edit::<Information>()?
            .interests
            .entry(r.person)
            .or_default();
        if let Some(old) = row.iter_mut().find(|i| i.topic == r.topic) {
            old.importance = r.importance;
        } else {
            if row.len() == 8 {
                row.remove(0);
            }
            row.push(Interest {
                topic: r.topic,
                importance: r.importance,
                asked: BTreeSet::new(),
                read: BTreeMap::new(),
            });
        }
        Ok(())
    })?;
    b.request::<Withdraw>(|tx, r| {
        if let Some(row) = tx.edit::<Information>()?.interests.get_mut(&r.person) {
            row.retain(|i| i.topic != r.topic);
        }
        Ok(())
    })?;
    b.request::<Ask>(|tx, r| {
        contact(tx, r.person, r.other)?;
        require(r.topic.valid(), "invalid question")?;
        if game_communication::active(tx) {
            require(
                game_communication::understood(tx, r.person, r.other, &r.topic.concept(), true)?,
                "question was not understood",
            )?;
        }
        if let Some(i) = tx
            .edit::<Information>()?
            .interests
            .get_mut(&r.person)
            .and_then(|row| row.iter_mut().find(|i| i.topic == r.topic))
            && i.asked.len() < 64
        {
            i.asked.insert(r.other);
        }
        if let Some(q) = tx
            .get::<Information>()?
            .inboxes
            .get(&r.other)
            .and_then(|row| {
                row.iter()
                    .find(|q| q.asker == r.person && q.topic == r.topic)
            })
        {
            return Ok(q.id);
        }
        let id = tx.allocate()?;
        let at = tx.now();
        let state = tx.edit::<Information>()?;
        let row = state.inboxes.entry(r.other).or_default();
        if row.len() == 16 {
            row.remove(0);
        }
        row.push(Question {
            id,
            asker: r.person,
            topic: r.topic.clone(),
            at,
        });
        tx.emit(
            Asked {
                person: r.person,
                other: r.other,
                topic: r.topic,
            },
            vec![r.person, r.other],
        )?;
        tx.notify(game_choice::Changed { person: r.other })?;
        Ok(id)
    })?;
    b.request::<Answer>(|tx, r| {
        let q = tx
            .get::<Information>()?
            .inboxes
            .get(&r.person)
            .and_then(|row| row.iter().find(|q| q.id == r.question))
            .cloned()
            .ok_or_else(|| Error::Rejected("question is no longer remembered".into()))?;
        contact(tx, r.person, q.asker)?;
        let declined = r.key.is_none();
        let mut shared = false;
        if let Some(key) = r.key {
            let facts = tx
                .get::<Knowledge>()?
                .0
                .get(&r.person)
                .ok_or_else(|| Error::Rejected("respondent knows no answer".into()))?;
            require(
                facts
                    .get(key.as_str())
                    .is_some_and(|f| q.topic.matches(&key, f.value)),
                "answer does not address the question",
            )?;
            let keys = topics::supporting(facts, &key, game_transport::area(tx.get()?, r.person)?);
            for key in keys {
                let result =
                    if key.starts_with("theory:") && tx.registered::<game_inquiry::Explain>() {
                        tx.attempt(game_inquiry::Explain {
                            teacher: r.person,
                            listener: q.asker,
                            key,
                            form: game_inquiry::Explanation::Practical,
                        })
                    } else {
                        tx.attempt(game_testimony::Tell {
                            sender: r.person,
                            recipient: q.asker,
                            key,
                        })
                        .map(|_| true)
                    };
                shared |= result.unwrap_or(false);
            }
        }
        tx.edit::<Information>()?
            .inboxes
            .get_mut(&r.person)
            .unwrap()
            .retain(|v| v.id != r.question);
        tx.emit(
            Answered {
                person: r.person,
                asker: q.asker,
                topic: q.topic,
                shared,
            },
            vec![r.person, q.asker],
        )?;
        Ok((shared || declined).into())
    })?;
    b.subscribe::<game_social_life::Encounter>("information.fresh-contact", |tx, e| {
        for (person, other) in [(e.first, e.second), (e.second, e.first)] {
            if let Some(row) = tx.edit::<Information>()?.interests.get_mut(&person) {
                for i in row {
                    i.asked.remove(&other);
                }
            }
        }
        Ok(())
    })?;
    b.subscribe::<game_physiology::Died>("information.death", |tx, e| {
        let state = tx.edit::<Information>()?;
        state.interests.remove(&e.person);
        state.inboxes.remove(&e.person);
        let people: Vec<_> = state.inboxes.keys().copied().collect();
        for p in people {
            state
                .inboxes
                .get_mut(&p)
                .unwrap()
                .retain(|q| q.asker != e.person);
        }
        Ok(())
    })
}
