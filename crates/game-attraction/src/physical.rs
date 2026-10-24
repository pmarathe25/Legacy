//! Per-review numeric projections, checked against live owner revision tokens.
use super::*;
use game_relationships::Social;
use game_storage::{Table, Version};
use std::{cell::RefCell, collections::BTreeMap, sync::Arc};
struct Frame {
    person: PersonId,
    at: SimTime,
    values: Option<Version<Arc<str>, i32>>,
    definitions: Option<Version<String, Arc<game_traits::Trait>>>,
    profiles: Option<Version<PersonId, Arc<BTreeMap<String, i32>>>>,
    lives: Option<Version<PersonId, game_vitality::Life>>,
    ideals: [i64; 2],
    sensitivity: i64,
    apparent: BTreeMap<PersonId, [i64; 2]>,
}
thread_local! {
    static FRAMES: RefCell<Box<[Option<Frame>;128]>> =
        RefCell::new(Box::new(std::array::from_fn(|_| None)));
}
fn ideals(tx: &Transaction<'_>, p: PersonId, s: &Social) -> [i64; 2] {
    [("size", "taste-size"), ("strength", "taste-build")].map(|(feature, locus)| {
        (game_vitality::disposition(tx, p, locus)
            + i64::from(s.opinion(p, &format!("beauty:{feature}"))) * 5)
            .clamp(0, 1000)
    })
}
fn apparent(tx: &Transaction<'_>, q: PersonId) -> [i64; 2] {
    ["size", "strength"]
        .map(|feature| (game_vitality::ability(tx, q, feature) / 200 * 100).min(1000) as i64)
}
fn visible(tx: &Transaction<'_>, p: PersonId, q: PersonId) -> bool {
    game_transport::area(tx.get().unwrap(), p)
        .ok()
        .is_some_and(|a| game_transport::area(tx.get().unwrap(), q).ok() == Some(a))
}
fn score(ideals: [i64; 2], apparent: [i64; 2], sensitivity: i64) -> i64 {
    ideals
        .into_iter()
        .zip(apparent)
        .map(|(i, a)| 50 - (i - a).abs() / 5)
        .sum::<i64>()
        * sensitivity
        / 1000
}
pub(super) fn uncached(tx: &Transaction<'_>, p: PersonId, q: PersonId, s: &Social) -> i64 {
    if !visible(tx, p, q) {
        return 0;
    }
    score(
        ideals(tx, p, s),
        apparent(tx, q),
        game_vitality::disposition(tx, p, "aesthetic-sensitivity"),
    )
}
pub(super) fn appearance(tx: &Transaction<'_>, p: PersonId, q: PersonId, s: &Social) -> i64 {
    if !visible(tx, p, q) {
        return 0;
    }
    let _query = game_diagnostics::span("appraisal physical query");
    let values = s.values.get(&p);
    let traits = tx.get::<game_traits::Traits>().ok();
    let vitality = tx.get::<game_vitality::Vitality>().ok();
    FRAMES.with(|frames| {
        let mut frames = frames.borrow_mut();
        let slot = &mut frames[p.0 as usize % 128];
        if !slot.as_ref().is_some_and(|v| {
            v.person == p
                && v.at == tx.now()
                && appraisal::same(&v.values, values)
                && match (traits, &v.definitions, &v.profiles) {
                    (Some(t), Some(d), Some(p)) => t.definitions.matches(d) && t.people.matches(p),
                    (None, None, None) => true,
                    _ => false,
                }
                && match (vitality, &v.lives) {
                    (Some(l), Some(v)) => v.matches(&l.people),
                    (None, None) => true,
                    _ => false,
                }
        }) {
            let _refresh = game_diagnostics::span("appraisal physical invalidated");
            *slot = Some(Frame {
                person: p,
                at: tx.now(),
                values: values.map(Table::version),
                definitions: traits.map(|t| t.definitions.version()),
                profiles: traits.map(|t| t.people.version()),
                lives: vitality.map(|l| l.people.version()),
                ideals: ideals(tx, p, s),
                sensitivity: game_vitality::disposition(tx, p, "aesthetic-sensitivity"),
                apparent: BTreeMap::new(),
            });
        }
        let view = slot.as_mut().unwrap();
        let physical = if let Some(value) = view.apparent.get(&q) {
            *value
        } else {
            let _compute = game_diagnostics::span("appraisal physical computed");
            let value = apparent(tx, q);
            if view.apparent.len() >= 64 {
                view.apparent.clear();
            }
            view.apparent.insert(q, value);
            value
        };
        score(view.ideals, physical, view.sensitivity)
    })
}
