//! Reports carry the sender's actual evidence and provenance, never a query of remote reality.
use game_core::*;
use game_knowledge::{Fact, Knowledge};
use game_sim::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize)]
pub struct Report {
    pub sender: PersonId,
    pub recipient: PersonId,
    pub key: String,
    pub delay: SimDuration,
}
impl Request for Report {
    const KEY: &'static str = "knowledge.report";
    type Response = SimTime;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Tell {
    pub sender: PersonId,
    pub recipient: PersonId,
    pub key: String,
}
impl Request for Tell {
    const KEY: &'static str = "knowledge.tell";
    type Response = SimTime;
}
#[derive(Clone, Serialize, Deserialize)]
struct Deliver {
    recipient: PersonId,
    key: String,
    fact: Fact,
    #[serde(default)]
    support: u64,
}
impl Request for Deliver {
    const KEY: &'static str = "knowledge.deliver";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Explain {
    pub sender: PersonId,
    pub recipient: PersonId,
    pub key: String,
    pub support: u64,
    #[serde(default)]
    pub demonstrate: bool,
}
impl Request for Explain {
    const KEY: &'static str = "testimony.explain";
    type Response = SimTime;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Comprehend {
    pub person: PersonId,
    pub fact: Fact,
    pub support: u64,
}
impl Request for Comprehend {
    const KEY: &'static str = "testimony.comprehend";
    type Response = bool;
}
fn send(
    tx: &mut Transaction<'_>,
    sender: PersonId,
    recipient: PersonId,
    key: String,
    delay: SimDuration,
    support: u64,
) -> Result<SimTime> {
    if tx.registered::<game_memory::Recall>() {
        tx.call(game_memory::Recall {
            person: sender,
            key: key.clone(),
        })?;
    }
    let mut fact = tx
        .get::<Knowledge>()?
        .0
        .get(&sender)
        .and_then(|m| m.get(key.as_str()))
        .map(|f| f.to_owned())
        .ok_or_else(|| Error::Rejected("sender does not know this fact".into()))?;
    fact.source = sender;
    fact.confidence = fact.confidence.min(90);
    let at = tx.now().after(delay)?;
    tx.schedule(
        at,
        30,
        Deliver {
            recipient,
            key,
            fact,
            support,
        },
    )?;
    Ok(at)
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("testimony", 1)?;
    b.request::<Tell>(|tx, r| {
        require(
            game_transport::area(tx.get()?, r.sender)?
                == game_transport::area(tx.get()?, r.recipient)?,
            "recipient is elsewhere",
        )?;
        if game_communication::active(tx) {
            require(
                game_communication::understood(tx, r.sender, r.recipient, &r.key, true)?,
                "report was not understood",
            )?;
        }
        tx.call(Report {
            sender: r.sender,
            recipient: r.recipient,
            key: r.key,
            delay: SimDuration(0),
        })
    })?;
    b.request::<Explain>(|tx, r| {
        require(
            game_transport::area(tx.get()?, r.sender)?
                == game_transport::area(tx.get()?, r.recipient)?,
            "explanation needs contact",
        )?;
        if game_communication::active(tx) {
            require(
                if r.demonstrate {
                    game_communication::illustrate(tx, r.sender, r.recipient, &r.key)?
                } else {
                    game_communication::understood(tx, r.sender, r.recipient, &r.key, true)?
                },
                "explanation was not understood",
            )?;
        }
        send(
            tx,
            r.sender,
            r.recipient,
            r.key,
            SimDuration(0),
            r.support.min(16000),
        )
    })?;
    b.request::<Report>(|tx, r| send(tx, r.sender, r.recipient, r.key, r.delay, 0))?;
    b.request::<Deliver>(|tx, r| {
        if tx.registered::<Comprehend>()
            && !tx.call(Comprehend {
                person: r.recipient,
                fact: r.fact.clone(),
                support: r.support,
            })?
        {
            return Ok(());
        }
        let previous = tx
            .get::<Knowledge>()?
            .0
            .get(&r.recipient)
            .and_then(|facts| facts.get(r.key.as_str()))
            .map(|f| f.to_owned());
        tx.call(game_knowledge::Receive {
            person: r.recipient,
            key: r.key.clone(),
            fact: r.fact,
        })?;
        let current = tx
            .get::<Knowledge>()?
            .0
            .get(&r.recipient)
            .and_then(|facts| facts.get(r.key.as_str()))
            .map(|f| f.to_owned());
        if current != previous {
            tx.notify(game_choice::Changed {
                person: r.recipient,
            })?;
        }
        Ok(())
    })
}

/// Comprehensible meanings in a remembered performance or text retain their original evidence.
pub fn absorb<'a>(
    tx: &mut Transaction<'_>,
    reader: PersonId,
    records: &[(String, Fact)],
    meanings: impl Iterator<Item = &'a str>,
) -> Result<()> {
    let understood: std::collections::BTreeSet<_> = meanings.collect();
    for (key, fact) in records {
        if !understood.contains(key.as_str()) {
            continue;
        }
        if tx.registered::<Comprehend>()
            && !tx.call(Comprehend {
                person: reader,
                fact: fact.clone(),
                support: 0,
            })?
        {
            continue;
        }
        tx.call(game_knowledge::Receive {
            person: reader,
            key: key.clone(),
            fact: fact.clone(),
        })?;
    }
    Ok(())
}
