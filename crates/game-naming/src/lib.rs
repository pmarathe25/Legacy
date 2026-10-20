//! Personal name choices use heard names, relationships, and experienced deeds.
use game_core::*;
use game_names::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
mod choices;
fn disposition(tx: &Transaction<'_>, p: PersonId, key: &str) -> i64 {
    game_vitality::disposition(tx, p, key)
}
fn affection(tx: &Transaction<'_>, p: PersonId, other: PersonId) -> i64 {
    tx.get::<game_relationships::Social>()
        .ok()
        .and_then(|s| s.relationships.get(&p)?.get(&other))
        .map_or(0, |r| i64::from(r.affection) - i64::from(r.grievance))
}
fn first(name: &str) -> &str {
    name.split_whitespace().next().unwrap_or(name)
}
fn invent(tx: &mut Transaction<'_>, chooser: PersonId, child: PersonId) -> Result<String> {
    if game_communication::active(tx) {
        return tx.call(game_communication::Coin {
            person: chooser,
            salt: child.0,
        });
    }
    let names = tx.get::<Names>()?;
    let known: Vec<_> = names
        .heard
        .get(&chooser)
        .into_iter()
        .flat_map(|row| row.values())
        .collect();
    require(!known.is_empty(), "no known name sounds")?;
    let draw = random(tx.seed(), chooser.0, child.0.wrapping_add(883));
    let a = first(known[(draw % known.len() as u64) as usize]);
    let b = first(known[((draw / 17) % known.len() as u64) as usize]);
    let prefix_len = 1 + ((draw / 31) % a.chars().count().max(1) as u64) as usize;
    let suffix_start = ((draw / 101) % b.chars().count().max(1) as u64) as usize;
    let prefix: String = a.chars().take(prefix_len).collect();
    let suffix: String = b
        .chars()
        .skip(suffix_start)
        .flat_map(char::to_lowercase)
        .collect();
    Ok(format!("{prefix}{suffix}").chars().take(80).collect())
}
fn contextual(
    tx: &mut Transaction<'_>,
    speaker: PersonId,
    person: PersonId,
    kind: Kind,
    descriptor: &str,
) -> Result<()> {
    if speaker == person
        || !tx.get::<Names>()?.enabled
        || game_physiology::capable(tx, speaker).is_err()
    {
        return Ok(());
    }
    let Some(name) = tx
        .get::<Names>()?
        .heard
        .get(&speaker)
        .and_then(|row| row.get(&person))
    else {
        return Ok(());
    };
    // A speaker may keep a familiar address instead of adopting a new description.
    if tx
        .get::<Names>()?
        .usage
        .get(&speaker)
        .and_then(|row| row.get(&person))
        .is_some_and(|aliases| aliases.iter().any(|a| a.kind == kind))
    {
        return Ok(());
    }
    let name = name.clone();
    let translated = if game_communication::active(tx) {
        Some(game_communication::render(&tx.call(
            game_communication::WordFor {
                person: speaker,
                concept: format!("descriptor:{descriptor}"),
            },
        )?))
    } else {
        None
    };
    let descriptor = translated.as_deref().unwrap_or(descriptor);
    let name = if kind == Kind::Title {
        format!("{descriptor} {}", first(&name))
    } else {
        format!("{} {descriptor}", first(&name))
    };
    if name.chars().count() > 80 {
        return Ok(());
    }
    tx.call(Label {
        speaker,
        person,
        name,
        kind,
    })
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Reconsider {
    pub person: PersonId,
}
impl Request for Reconsider {
    const KEY: &'static str = "naming.reconsider";
    type Response = bool;
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("naming", 1)?;
    b.request::<SuggestChild>(choices::suggest)?;
    b.request::<Reconsider>(|tx, r| {
        if !tx.get::<Names>()?.enabled
            || game_physiology::capable(tx, r.person).is_err()
            || tx
                .get::<game_lineage::Lineage>()
                .is_ok_and(|s| s.current == Some(r.person))
        {
            return Ok(false);
        }
        let Some(life) = tx
            .get::<game_vitality::Vitality>()
            .ok()
            .and_then(|v| v.people.get(&r.person))
        else {
            return Ok(false);
        };
        if game_vitality::maturation(life.born, tx.now(), "foresight") < 650 {
            return Ok(false);
        }
        let names = tx.get::<Names>()?;
        let identity = names.identities[&r.person].clone();
        let current = identity.changes.last().unwrap();
        let inertia = 500
            + disposition(tx, r.person, "patience") / 2
            + ((tx.now().0.saturating_sub(current.0.0) / (365 * DAY.0)).min(10) * 20) as i64
            + affection(tx, r.person, identity.chosen_by).max(0);
        let creative = disposition(tx, r.person, "creativity");
        let ambition = disposition(tx, r.person, "ambition");
        let support = |speaker| affection(tx, r.person, speaker) * 4 + creative / 2 + ambition / 4;
        let incumbent = names
            .offers
            .get(&r.person)
            .into_iter()
            .flat_map(|row| row.iter())
            .filter(|(_, name)| *name == &current.1)
            .map(|(speaker, _)| support(*speaker))
            .max()
            .unwrap_or(0);
        // Compare the present name too: equally appealing offers need not cause oscillation.
        let switching_cost = 25
            + disposition(tx, r.person, "patience") / 10
            + disposition(tx, r.person, "rigidity") / 5;
        let mut candidate = names
            .offers
            .get(&r.person)
            .into_iter()
            .flat_map(|row| row.iter())
            .filter(|(_, name)| *name != &current.1)
            .map(|(speaker, name)| (support(*speaker), name.clone()))
            .max_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.cmp(&a.1)));
        // Self-fashioning competes with attachment to the existing identity.
        if identity.changes.len() == 1 && creative + ambition > 1450 {
            let own = invent(tx, r.person, r.person)?;
            let score =
                (creative + ambition) / 2 - affection(tx, r.person, identity.chosen_by).min(0) * 3;
            if own != current.1 && candidate.as_ref().is_none_or(|(best, _)| score > *best) {
                candidate = Some((score, own));
            }
        }
        if let Some((score, name)) = candidate
            && score > inertia
            && score > incumbent + switching_cost
        {
            tx.call(Change {
                person: r.person,
                name,
            })?;
            return Ok(true);
        }
        Ok(false)
    })?;
    b.subscribe::<game_social_life::Encounter>("naming.familiarity", |tx, e| {
        if !tx.get::<Names>()?.enabled {
            return Ok(());
        }
        for (speaker, person) in [(e.first, e.second), (e.second, e.first)] {
            tx.call(LearnName {
                listener: speaker,
                person,
            })?;
            if affection(tx, speaker, person) > 20 && disposition(tx, speaker, "creativity") > 300 {
                let words = [
                    "dearheart",
                    "bright one",
                    "old friend",
                    "companion",
                    "quickwit",
                ];
                let word = words[(random(tx.seed(), speaker.0, person.0.wrapping_add(980))
                    % words.len() as u64) as usize];
                contextual(tx, speaker, person, Kind::Nickname, word)?;
            }
            let title = tx
                .get::<game_institutions::Institutions>()
                .ok()
                .and_then(|s| {
                    s.groups
                        .values()
                        .filter(|g| g.members.contains(&speaker))
                        .find_map(|g| {
                            g.offices
                                .iter()
                                .find(|(_, holder)| **holder == person)
                                .map(|(office, _)| office.clone())
                        })
                });
            if let Some(title) = title
                && affection(tx, speaker, person) + disposition(tx, speaker, "patience") / 20 > 20
            {
                contextual(tx, speaker, person, Kind::Title, &title)?;
            }
            tx.call(Reconsider { person: speaker })?;
        }
        Ok(())
    })?;
    b.subscribe::<game_health::Treated>("naming.gratitude", |tx, e| {
        if e.successful && tx.get::<Names>()?.enabled && e.patient != e.caregiver {
            let heard = tx
                .attempt(LearnName {
                    listener: e.patient,
                    person: e.caregiver,
                })
                .is_ok();
            if heard && disposition(tx, e.patient, "empathy") > 250 {
                contextual(tx, e.patient, e.caregiver, Kind::Epithet, "the Kind")?;
            }
        }
        Ok(())
    })?;
    b.subscribe::<game_conflict::Struck>("naming.violence", |tx, e| {
        if e.amount > 0
            && tx.get::<Names>()?.enabled
            && game_physiology::capable(tx, e.target).is_ok()
        {
            let heard = tx
                .attempt(LearnName {
                    listener: e.target,
                    person: e.actor,
                })
                .is_ok();
            if heard && disposition(tx, e.target, "aggression") > 250 {
                contextual(tx, e.target, e.actor, Kind::Epithet, "the Cruel")?;
            }
        }
        Ok(())
    })?;
    b.subscribe::<game_works::Shared>("naming.reputation", |tx, e| {
        if !tx.get::<Names>()?.enabled {
            return Ok(());
        }
        let heard = tx
            .attempt(LearnName {
                listener: e.listener,
                person: e.speaker,
            })
            .is_ok();
        if heard && disposition(tx, e.listener, "creativity") > 350 {
            contextual(tx, e.listener, e.speaker, Kind::Title, "Teller")?;
        }
        Ok(())
    })?;
    Ok(())
}
