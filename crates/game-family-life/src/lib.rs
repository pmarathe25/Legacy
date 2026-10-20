//! Family choices, child provisioning and consensual accompaniment compose household records.
use game_core::*;
use game_households::Households;
use game_sim::*;
mod bonds;
pub use bonds::{Bond, Bonds, Kind, Separate, ShareCare};
pub use game_attraction::attraction;
fn mobile(tx: &Transaction<'_>, child: PersonId) -> bool {
    if tx
        .get::<game_vitality::Vitality>()
        .is_ok_and(|v| v.people.contains_key(&child))
    {
        return game_vitality::can_walk(tx, child);
    }
    tx.get::<game_physiology::Bodies>()
        .ok()
        .and_then(|b| b.people.get(&child))
        .is_some_and(|b| {
            game_vitality::maturation(b.born, tx.now(), "speed").saturating_mul(
                game_vitality::factor(game_vitality::ability(tx, child, "speed")),
            ) / 1000
                >= 200
        })
}
/// Anticipated dependence uses personal needs and remembered meals, not the parent's private stock.
pub fn dependence(tx: &Transaction<'_>, child: PersonId) -> i64 {
    let need = tx
        .get::<game_physiology::Bodies>()
        .ok()
        .and_then(|b| b.people.get(&child))
        .map_or(0, |b| b.hunger.saturating_add(b.fatigue).min(200) as i64);
    let known_meal = tx
        .get::<game_knowledge::Knowledge>()
        .ok()
        .and_then(|k| k.0.get(&child))
        .is_some_and(|facts| {
            facts.iter().any(|(key, fact)| {
                key.starts_with("store:")
                    && tx
                        .get::<game_objects::Objects>()
                        .is_ok_and(|o| game_provisioning::remembered_meal(o, fact.value))
            })
        });
    let vulnerability = need
        + if !game_vitality::can_manipulate(tx, child) {
            200
        } else if !known_meal {
            60
        } else {
            0
        };
    let foresight = game_vitality::ability(tx, child, "foresight").min(16000) as i64;
    vulnerability * (1000 + foresight) / (2000 + foresight)
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("family-life", 2)?;
    game_attraction::plugin(b)?;
    bonds::register(b)?;
    b.subscribe::<game_lifecycle::Born>("c.family-life.provision", |tx, e| {
        if !tx.get::<Households>()?.enabled {
            return Ok(());
        }
        let f = tx.get::<game_lifecycle::Population>()?.families[&e.person].clone();
        let t = tx.get::<game_traits::Traits>()?;
        let capacity =
            (500 + t.ability(e.person, "size") + t.ability(e.person, "strength") / 2).max(1200);
        let inventory = tx.call(game_cargo::Create {
            custody: game_cargo::Custody::Carried(e.person),
            capacity,
        })?;
        tx.call(game_property::ClaimInventory {
            claimant: e.person,
            inventory,
            recognized_by: std::collections::BTreeSet::from([e.person]),
        })?;
        tx.call(game_property::Permit {
            issuer: e.person,
            person: e.person,
            inventory,
            allow: true,
        })?;
        tx.call(game_knowledge::ObserveStore {
            person: e.person,
            inventory,
        })?;
        if tx.registered::<game_names::NameChild>() {
            tx.call(game_names::NameChild { child: e.person })?;
        } else {
            let name = tx
                .get::<game_social_life::SocialLife>()?
                .people
                .get(&f.attendant)
                .map_or("Kin", |p| p.name.as_str())
                .to_owned();
            tx.call(game_social_life::Register {
                person: e.person,
                name,
            })?;
        }
        tx.call(game_relationships::Initialize {
            person: e.person,
            beliefs: Default::default(),
            values: Default::default(),
        })?;
        tx.call(game_characters::Register {
            person: e.person,
            mind: Default::default(),
        })?;
        for p in [f.mother, f.father] {
            tx.call(game_relationships::Converse {
                observer: p,
                other: e.person,
                warmth: 10,
            })?;
        }
        Ok(())
    })?;
    b.subscribe::<game_transport::Departed>("households.carry", |tx, r| {
        let households = tx.get::<Households>()?;
        let mut children: Vec<_> = households
            .beneficiaries_of(r.person)
            .chain(
                households
                    .children
                    .get(&r.person)
                    .into_iter()
                    .flatten()
                    .copied(),
            )
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        children.sort_by_key(|child| {
            (
                mobile(tx, *child),
                game_vitality::ability(tx, *child, "size"),
            )
        });
        for child in children {
            let attachment = tx
                .get::<game_relationships::Social>()?
                .relationships
                .get(&r.person)
                .and_then(|row| row.get(&child))
                .map_or(0, |r| r.affection);
            let empathy = game_vitality::disposition(tx, r.person, "empathy");
            let altruism = game_vitality::disposition(tx, r.person, "altruism");
            if i64::from(attachment) * 5 + empathy * altruism / 1000 < 100 {
                continue;
            }
            if mobile(tx, child) {
                let tie = tx
                    .get::<game_relationships::Social>()?
                    .relationships
                    .get(&child)
                    .and_then(|row| row.get(&r.person))
                    .cloned()
                    .unwrap_or_default();
                let independent = bonds::value(tx, child, "independence")
                    + game_vitality::disposition(tx, child, "risk-tolerance") / 20;
                if i64::from(tie.affection + tie.trust - tie.grievance - tie.fear)
                    + dependence(tx, child)
                    <= independent
                {
                    continue;
                }
            }
            if tx
                .get::<Actions>()?
                .0
                .get(&child)
                .is_some_and(|a| a.kind == "socialize")
            {
                tx.cancel_action(child)?;
            }
            if game_physiology::capable(tx, child).is_ok()
                && tx.get::<Actions>()?.0.get(&child).is_none_or(|a| {
                    ["rest", "express-need"].contains(&a.kind.as_str())
                        || !mobile(tx, child) && a.kind != "combat"
                })
            {
                let _ = tx.attempt(game_transport::Carry {
                    carrier: r.person,
                    passenger: child,
                    destination: r.destination,
                });
            }
        }
        Ok(())
    })?;
    Ok(())
}
