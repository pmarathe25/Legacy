//! Experienced commitments affect switching costs; no action or belief is compulsory.
use game_core::*;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Commitment {
    pub preference: String,
    pub confirmations: u16,
    pub at: SimTime,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    pub people: Table<PersonId, Table<String, Commitment>>,
}
impl Resource for Identity {
    const KEY: &'static str = "identity.commitments";
    fn validate(&self) -> Result<()> {
        require(
            self.people.values().all(|row| {
                row.len() <= 64
                    && row.iter().all(|(key, c)| {
                        !key.is_empty()
                            && key.len() <= 160
                            && !c.preference.is_empty()
                            && c.preference.len() <= 256
                            && (1..=1000).contains(&c.confirmations)
                    })
            }),
            "invalid personal commitment",
        )
    }
    fn validate_changes(&self, old: &Self) -> Result<()> {
        for (_, row) in self.people.changed(&old.people) {
            require(
                row.len() <= 64
                    && row.iter().all(|(key, c)| {
                        !key.is_empty()
                            && key.len() <= 160
                            && !c.preference.is_empty()
                            && c.preference.len() <= 256
                            && (1..=1000).contains(&c.confirmations)
                    }),
                "invalid personal commitment",
            )?;
        }
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Commit {
    pub person: PersonId,
    pub domain: String,
    pub preference: String,
}
impl Request for Commit {
    const KEY: &'static str = "identity.commit";
    type Response = ();
}
/// A bounded preference cost; stronger evidence or urgent needs can outweigh it.
pub fn resistance(tx: &Transaction<'_>, p: PersonId, domain: &str, alternative: &str) -> i64 {
    let Some(c) = tx
        .get::<Identity>()
        .ok()
        .and_then(|s| s.people.get(&p)?.get(domain))
    else {
        return 0;
    };
    if c.preference == alternative {
        return 0;
    }
    let rigidity = game_vitality::disposition(tx, p, "rigidity");
    rigidity * (20 + i64::from(c.confirmations.min(80))) / 1000
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("identity", 1)?;
    b.resource::<Identity>()?;
    b.request::<Commit>(|tx, r| {
        require(
            !r.domain.is_empty()
                && r.domain.len() <= 160
                && !r.preference.is_empty()
                && r.preference.len() <= 256,
            "invalid commitment",
        )?;
        if tx
            .get::<Identity>()?
            .people
            .get(&r.person)
            .and_then(|row| row.get(&r.domain))
            .is_some_and(|c| c.preference == r.preference && c.confirmations == 1000)
        {
            return Ok(());
        }
        let now = tx.now();
        let row = tx.edit::<Identity>()?.people.entry(r.person).or_default();
        let confirmations = row
            .get(&r.domain)
            .filter(|c| c.preference == r.preference)
            .map_or(1, |c| c.confirmations.saturating_add(1).min(1000));
        if row.len() == 64 && !row.contains_key(&r.domain) {
            let oldest = row
                .iter()
                .min_by_key(|(key, c)| (c.at, *key))
                .unwrap()
                .0
                .clone();
            row.remove(&oldest);
        }
        row.insert(
            r.domain,
            Commitment {
                preference: r.preference,
                confirmations,
                at: now,
            },
        );
        Ok(())
    })?;
    Ok(())
}
