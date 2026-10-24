//! Checked owner inputs avoid repeated row resolution and raw-trait projection within a review.
use super::*;
use game_storage::Version;
use std::{cell::RefCell, collections::BTreeMap, sync::Arc};
struct Frame {
    person: PersonId,
    at: SimTime,
    life: Option<Version<PersonId, Life>>,
    definitions: Option<Version<String, Arc<game_traits::Trait>>>,
    profiles: Option<Version<PersonId, Arc<BTreeMap<String, i32>>>>,
    expression: Option<game_expression::Projection>,
    seen: u32,
    values: [u64; 28],
}
thread_local! {
    static RECENT: RefCell<Box<[Option<Frame>; 128]>> =
        RefCell::new(Box::new(std::array::from_fn(|_| None)));
}
fn remember(
    tx: &Transaction<'_>,
    person: PersonId,
    slot: usize,
    compute: impl FnOnce(Option<&mut game_expression::Projection>, Option<&game_traits::Traits>) -> u64,
) -> u64 {
    let vitality = tx.get::<Vitality>().ok();
    let traits = tx.get::<game_traits::Traits>().ok();
    let at = tx.now();
    RECENT.with(|cache| {
        let mut cache = cache.borrow_mut();
        let entry = &mut cache[person.0 as usize % 128];
        let valid = entry.as_ref().is_some_and(|f| {
            f.person == person
                && f.at == at
                && match (vitality, &f.life) {
                    (Some(v), Some(version)) => version.matches(&v.people),
                    (None, None) => true,
                    _ => false,
                }
                && match (traits, &f.definitions, &f.profiles) {
                    (Some(t), Some(definitions), Some(profiles)) => {
                        t.definitions.matches(definitions) && t.people.matches(profiles)
                    }
                    (None, None, None) => true,
                    _ => false,
                }
        });
        if !valid {
            *entry = Some(Frame {
                person,
                at,
                life: vitality.map(|v| v.people.version()),
                definitions: traits.map(|t| t.definitions.version()),
                profiles: traits.map(|t| t.people.version()),
                expression: vitality
                    .and_then(|v| v.people.get(&person))
                    .map(|life| game_expression::Projection::new(*life, at)),
                seen: 0,
                values: [0; 28],
            });
        }
        let frame = entry.as_mut().unwrap();
        if frame.seen & (1 << slot) == 0 {
            frame.values[slot] = compute(frame.expression.as_mut(), traits);
            frame.seen |= 1 << slot;
        }
        frame.values[slot]
    })
}
pub(super) fn ability(tx: &Transaction<'_>, person: PersonId, key: &str) -> u64 {
    let Some(slot) = game_expression::ability_slot(key) else {
        return tx
            .get::<game_traits::Traits>()
            .map_or(1000, |t| t.ability(person, key));
    };
    remember(tx, person, slot, |life, traits| {
        let raw = traits.map_or(1000, |t| t.ability(person, key));
        life.map_or(raw, |v| v.ability(raw, key))
    })
}
pub(super) fn disposition(tx: &Transaction<'_>, person: PersonId, key: &str) -> i64 {
    let Some(slot) = game_expression::disposition_slot(key) else {
        return tx
            .get::<game_traits::Traits>()
            .map_or(500, |t| t.disposition(person, key));
    };
    remember(tx, person, slot, |life, traits| {
        let raw = traits.map_or(500, |t| t.disposition(person, key));
        life.map_or(raw, |v| v.disposition(raw, key)) as u64
    }) as i64
}
pub(super) fn readiness(tx: &Transaction<'_>, person: PersonId, key: &str) -> bool {
    let slot = match key {
        "speed" => 25,
        "agility" => 26,
        "language" => 27,
        _ => unreachable!("motor or conversation readiness"),
    };
    remember(tx, person, slot, |life, traits| {
        let Some(life) = life else { return 1 };
        let (ability, threshold) = if key == "language" {
            ("intelligence", 400)
        } else {
            (key, 200)
        };
        let raw = traits.map_or(1000, |t| t.ability(person, ability));
        u64::from(
            life.maturity(key)
                .saturating_mul(factor(life.ability(raw, ability)))
                / 1000
                >= threshold,
        )
    }) != 0
}
