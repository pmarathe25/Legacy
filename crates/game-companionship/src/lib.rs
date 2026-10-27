//! Initiators choose social opportunities; recipients retain their own consent.
use game_core::*;
use game_sim::*;
use game_social_life::{Scanned, SocialLife, Talk, affinity, company_motivation, ready};
use game_vitality::disposition;
use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize)]
pub struct SeekCompany {
    pub person: PersonId,
    pub protected: Option<PersonId>,
}
impl Request for SeekCompany {
    const KEY: &'static str = "social-life.seek";
    type Response = bool;
}
#[derive(Clone, Serialize, Deserialize)]
struct Approach(Talk);
impl Request for Approach {
    const KEY: &'static str = "companionship.approach";
    type Response = game_choice::Outcome;
}
fn options(tx: &Transaction<'_>, person: PersonId, protected: Option<PersonId>) -> Vec<PersonId> {
    let state = tx.get::<SocialLife>().expect("social state");
    let Some(p) = state.people.get(&person) else {
        return vec![];
    };
    let Some(area) = p.area else { return vec![] };
    let current = tx
        .get::<game_lineage::Lineage>()
        .ok()
        .and_then(|l| l.current);
    let residents = &state.residents[&area];
    let explore = random(tx.seed(), person.0, p.encounters.wrapping_add(901)) % 1000
        < ((disposition(tx, person, "sociability") + disposition(tx, person, "risk-tolerance")) / 4)
            as u64;
    let familiar = tx
        .get::<game_relationships::Social>()
        .ok()
        .and_then(|s| s.relationships.get(&person));
    let mut peers: Vec<_> = residents
        .scan_after(p.cursor.as_ref())
        .chain(residents.iter())
        .take(residents.len().min(16))
        .map(|(id, _)| *id)
        .filter(|other| {
            *other != person
                && Some(*other) != protected
                && Some(*other) != current
                && (!tx.get::<Actions>().unwrap().0.contains_key(other)
                    || state
                        .meetings
                        .get(&person)
                        .is_some_and(|m| m.other == *other))
        })
        .collect();
    peers.sort_by_key(|other| {
        std::cmp::Reverse((
            affinity(tx, person, *other) - if p.recent == Some(*other) { 10 } else { 0 }
                + if explore && familiar.is_none_or(|r| !r.contains_key(other)) {
                    200
                } else {
                    0
                },
            std::cmp::Reverse(*other),
        ))
    });
    peers
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("companionship", 1)?;
    b.request::<Approach>(|tx, r| {
        require(
            tx.get::<game_lineage::Lineage>()
                .ok()
                .and_then(|l| l.current)
                != Some(r.0.other),
            "player-controlled companion requires an invitation",
        )?;
        let result = tx.attempt(r.0.clone());
        tx.call(Scanned {
            person: r.0.person,
            through: r.0.other,
            failed: result.is_err(),
        })?;
        Ok(if result.is_ok() {
            game_choice::Outcome::Performed
        } else {
            game_choice::Outcome::Reconsider
        })
    })?;
    game_choice::provider(b, "social-life.propose", |tx, e| {
        if !game_choice::collecting(tx, e.person) || !game_vitality::can_converse(tx, e.person) {
            return Ok(());
        }
        let protected = tx
            .get::<game_lineage::Lineage>()
            .ok()
            .and_then(|l| l.current);
        let desire = company_motivation(tx, e.person) / 20;
        let mut peers: Vec<_> = options(tx, e.person, protected)
            .into_iter()
            .take(8)
            .collect();
        // The current conversation is already known, even outside the search window.
        if let Some(other) = tx
            .get::<SocialLife>()?
            .meetings
            .get(&e.person)
            .map(|m| m.other)
            && !peers.contains(&other)
        {
            peers.push(other);
        }
        for other in peers {
            let value = affinity(tx, e.person, other) / 10;
            let request = Approach(Talk {
                person: e.person,
                other,
            });
            let switching = tx
                .get::<SocialLife>()?
                .meetings
                .get(&e.person)
                .is_some_and(|m| m.other != other);
            let mut offer = if switching {
                game_choice::Proposal::new(
                    format!("company:{}", other.0),
                    game_choice::motive(0, desire, value, 1, 0),
                    game_choice::Undertake {
                        person: e.person,
                        kind: "approach-company".into(),
                        duration: MINUTE,
                        request: Pending::immediate(request),
                    },
                )?
            } else {
                // Talk already occupies both participants' time. Reserve a willing
                // partner now, rather than racing another delayed invitation.
                game_choice::Proposal::new(
                    format!("company:{}", other.0),
                    game_choice::motive(0, desire, value, 1, 0),
                    request,
                )?
            };
            if tx
                .get::<SocialLife>()?
                .meetings
                .get(&e.person)
                .is_some_and(|m| m.other == other)
            {
                offer.continuation = tx.get::<Actions>()?.0.get(&e.person).map(|a| a.id);
            }
            game_choice::submit(tx, e.person, offer)?;
        }
        Ok(())
    })?;
    b.request::<SeekCompany>(|tx, r| {
        if !ready(tx, r.person)
            || company_motivation(tx, r.person) <= 0
            || tx.get::<SocialLife>()?.people[&r.person].retry_at > tx.now()
        {
            return Ok(false);
        }
        for other in options(tx, r.person, r.protected) {
            if tx.call(Approach(Talk {
                person: r.person,
                other,
            }))? == game_choice::Outcome::Performed
            {
                return Ok(true);
            }
        }
        Ok(false)
    })
}
