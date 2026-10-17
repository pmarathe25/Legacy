//! Distinct works preserve the creator's actual personal evidence and local interpretations.
use game_core::*;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Work {
    pub creator: PersonId,
    pub area: AreaId,
    pub at: SimTime,
    pub title: String,
    pub activity: String,
    pub evidence: Vec<(String, game_knowledge::Fact)>,
    pub audience: Table<PersonId, i32>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Works {
    pub enabled: bool,
    pub pieces: Table<u64, Work>,
    pub next: Table<PersonId, SimTime>,
}
impl Resource for Works {
    const KEY: &'static str = "works.creations";
    fn allocated_ids(&self) -> Vec<u64> {
        self.pieces.keys().copied().collect()
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Enable;
impl Request for Enable {
    const KEY: &'static str = "works.enable";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Created {
    pub work: u64,
    pub creator: PersonId,
    pub title: String,
    pub audience: usize,
}
impl Event for Created {
    const KEY: &'static str = "works.created";
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("works", 1)?;
    b.resource::<Works>()?;
    b.request::<Enable>(|tx, _| {
        tx.edit::<Works>()?.enabled = true;
        Ok(())
    })?;
    b.subscribe::<game_pastimes::Experienced>("works.testimony", |tx, e| {
        if !e.completed
            || !tx.get::<Works>()?.enabled
            || tx
                .get::<Works>()?
                .next
                .get(&e.person)
                .is_some_and(|at| *at > tx.now())
            || game_vitality::disposition(tx, e.person, "creativity") < 350
        {
            return Ok(());
        }
        let activity = tx.get::<game_pastimes::Pastimes>()?.activities[&e.activity].clone();
        if !["writing", "music", "art", "storytelling"].contains(&activity.skill.as_str()) {
            return Ok(());
        }
        let Ok(area) = game_transport::area(tx.get()?, e.person) else {
            return Ok(());
        };
        let evidence: Vec<_> = tx
            .get::<game_knowledge::Knowledge>()?
            .0
            .get(&e.person)
            .into_iter()
            .flat_map(|f| f.iter())
            .filter(|(k, _)| k.starts_with("roads:") || k.starts_with("patch:"))
            .take(8)
            .map(|(k, f)| (k.to_string(), f.to_owned()))
            .collect();
        if evidence.is_empty() {
            return Ok(());
        }
        let title = format!("{}: {}", activity.name, evidence[0].0);
        let mut audience = Table::new();
        let peers: Vec<_> = tx
            .get::<game_social_life::SocialLife>()?
            .residents
            .get(&area)
            .into_iter()
            .flat_map(|r| r.keys())
            .filter(|p| **p != e.person)
            .take(32)
            .copied()
            .collect();
        for p in peers {
            let receptivity = game_vitality::disposition(tx, p, "contemplation");
            let honesty = tx
                .get::<game_relationships::Social>()?
                .beliefs
                .get(&p)
                .map_or(0, |b| b.honesty);
            let response =
                ((receptivity - 400) / 100 + i64::from(honesty) / 4).clamp(-10, 10) as i32;
            audience.insert(p, response);
            tx.call(game_relationships::LearnValue {
                person: p,
                concept: activity.concept.clone(),
                outcome: response,
            })?;
        }
        let count = audience.len();
        let id = tx.allocate()?;
        let now = tx.now();
        let next = now.after(SimDuration(365 * DAY.0))?;
        tx.edit::<Works>()?.pieces.insert(
            id,
            Work {
                creator: e.person,
                area,
                at: now,
                title: title.clone(),
                activity: e.activity.clone(),
                evidence,
                audience,
            },
        );
        tx.edit::<Works>()?.next.insert(e.person, next);
        tx.emit(
            Created {
                work: id,
                creator: e.person,
                title,
                audience: count,
            },
            vec![e.person],
        )
    })?;
    Ok(())
}
