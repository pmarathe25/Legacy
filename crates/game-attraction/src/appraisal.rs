//! An incremental join of one observer's own values and personally heard opinions.
use super::*;
use game_relationships::{Beliefs, Opinion, Social};
use game_storage::{Table, Version};
use std::{cell::RefCell, collections::BTreeMap, sync::Arc};

pub(super) fn same<K, V>(version: &Option<Version<K, V>>, row: Option<&Table<K, V>>) -> bool {
    match (version, row) {
        (Some(v), Some(r)) => v.matches(r),
        (None, None) => true,
        _ => false,
    }
}
struct View {
    person: PersonId,
    values: Option<Version<Arc<str>, i32>>,
    heard: Option<Version<PersonId, Table<Arc<str>, Opinion>>>,
    beliefs: Option<Beliefs>,
    scores: BTreeMap<PersonId, i64>,
}
thread_local! {
    static VIEWS: RefCell<Box<[Option<View>; 1024]>> =
        RefCell::new(Box::new(std::array::from_fn(|_| None)));
}
pub(super) fn emotional(s: &Social, p: PersonId, q: PersonId) -> i64 {
    s.relationships
        .get(&p)
        .and_then(|r| r.get(&q))
        .map_or(0, |r| {
            i64::from(r.affection) * 2 + i64::from(r.trust) + i64::from(r.admiration) / 2
                - i64::from(r.fear) * 3
                - i64::from(r.grievance) * 3
        })
}
pub(super) fn uncached(s: &Social, p: PersonId, q: PersonId) -> i64 {
    let mut result = 0;
    if let Some(heard) = s.heard.get(&p).and_then(|r| r.get(&q)) {
        let mut count = 0;
        for (concept, opinion) in heard {
            if opinion.coerced || concept.starts_with("claim:") {
                continue;
            }
            let own = s.opinion(p, concept);
            if own == 0 {
                continue;
            }
            result +=
                i64::from(own.abs() + opinion.stance.abs() - (own - opinion.stance).abs()) / 2;
            count += 1;
            if count == 8 {
                break;
            }
        }
    }
    result.clamp(-200, 200)
}
pub(super) fn compatibility(s: &Social, p: PersonId, q: PersonId) -> i64 {
    let _query = game_diagnostics::span("appraisal compatibility query");
    let heard = s.heard.get(&p);
    if heard.and_then(|row| row.get(&q)).is_none() {
        return 0;
    }
    let values = s.values.get(&p);
    let beliefs = s.beliefs.get(&p);
    if values.is_none() && beliefs.is_none() {
        return 0;
    }
    VIEWS.with(|views| {
        let mut views = views.borrow_mut();
        let slot = &mut views[p.0 as usize % 1024];
        if !slot.as_ref().is_some_and(|v| {
            v.person == p
                && same(&v.values, values)
                && same(&v.heard, heard)
                && v.beliefs.as_ref() == beliefs
        }) {
            let _refresh = game_diagnostics::span("appraisal compatibility invalidated");
            *slot = Some(View {
                person: p,
                values: values.map(Table::version),
                heard: heard.map(Table::version),
                beliefs: beliefs.cloned(),
                scores: BTreeMap::new(),
            });
        }
        let view = slot.as_mut().unwrap();
        if let Some(score) = view.scores.get(&q) {
            return *score;
        }
        let _compute = game_diagnostics::span("appraisal compatibility computed");
        let score = uncached(s, p, q);
        if view.scores.len() >= 64 {
            view.scores.clear();
        }
        view.scores.insert(q, score);
        score
    })
}

#[cfg(test)]
mod tests;
