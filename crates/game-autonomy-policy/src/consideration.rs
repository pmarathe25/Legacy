//! An explicit model of temporarily retaining considered non-survival options.
use game_choice::Proposal;
use game_core::*;
use game_sim::Transaction;
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

pub struct Options {
    pub offers: Vec<Proposal>,
    pub food: Result<Vec<game_provisioning::Opportunity>>,
    pub reused: bool,
    pub remembered: Option<Arc<Consideration>>,
}
pub fn consider(
    tx: &mut Transaction<'_>,
    person: PersonId,
    rules: &crate::Rules,
    memory: Option<&Consideration>,
    fresh: bool,
) -> Result<Options> {
    let body = &tx.get::<game_physiology::Bodies>()?.people[&person];
    let remembered = (!fresh && rules.option_memory.is_some() && body.hunger < rules.urgent_hunger)
        .then(|| memory?.recall(tx.now()))
        .flatten();
    let meals = rules.gathering_meals;
    if let Some(offers) = remembered {
        let _measurement = game_diagnostics::span("considered options reused");
        let updated = if let Some(memory) = memory.filter(|m| m.pending.is_none()) {
            let mut updated = memory.clone();
            updated.arm(tx, person)?;
            Some(Arc::new(updated))
        } else {
            None
        };
        return Ok(Options {
            offers,
            food: game_decision_food::options(tx, person, meals),
            reused: true,
            remembered: updated,
        });
    }
    let _measurement = game_diagnostics::span("considered options refreshed");
    if let Some(memory) = memory {
        memory.cancel(tx);
    }
    let (offers, food) = game_choice::collect_with(tx, person, |view| {
        game_decision_food::options(view, person, meals)
    })?;
    let remembered = if let Some(span) = rules.option_memory {
        let weights = tx
            .get::<game_traits::Traits>()
            .ok()
            .map(|t| t.weights(person));
        Some(Arc::new(Consideration::shortlist(
            tx.now(),
            span,
            &offers,
            rules.option_limit,
            |term| {
                weights
                    .as_ref()
                    .and_then(|w| w.get(term))
                    .copied()
                    .unwrap_or(100)
            },
        )?))
    } else {
        None
    };
    Ok(Options {
        offers,
        food,
        reused: false,
        remembered,
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Consideration {
    pub(crate) until: SimTime,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) pending: Option<ScheduleId>,
    offers: Arc<Vec<Proposal>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    sources: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    used: BTreeSet<String>,
}
impl Consideration {
    pub fn shortlist(
        now: SimTime,
        span: SimDuration,
        offers: &[Proposal],
        limit: usize,
        weight: impl Fn(&str) -> i64,
    ) -> Result<Self> {
        require(
            limit > 0 && limit <= game_choice::LIMIT,
            "invalid option memory capacity",
        )?;
        let mut ranked: Vec<_> = offers
            .iter()
            .enumerate()
            .filter(|(_, p)| p.continuation.is_none())
            .map(|(i, p)| {
                let mut candidate = p.motive;
                candidate.reweight(&weight);
                (i, (candidate.score(), Reverse(candidate.id), &p.key))
            })
            .collect();
        ranked.sort_unstable_by_key(|(_, priority)| Reverse(*priority));
        ranked.truncate(limit);
        // Keep the collection's original key order for subsequent stable ties.
        ranked.sort_unstable_by_key(|(index, _)| *index);
        let selected: Vec<_> = ranked.into_iter().map(|(i, _)| offers[i].clone()).collect();
        Self::remember(now, span, &selected)
    }
    pub fn remember(now: SimTime, span: SimDuration, offers: &[Proposal]) -> Result<Self> {
        require(span.0 > 0, "empty option memory")?;
        serde_json::to_writer(std::io::sink(), offers)
            .map_err(|e| Error::InvalidSave(e.to_string()))?;
        // Action-specific continuations must never survive into another action.
        Ok(Self {
            until: now.after(span)?,
            pending: None,
            offers: Arc::new(
                offers
                    .iter()
                    .filter(|p| p.continuation.is_none())
                    .cloned()
                    .collect(),
            ),
            sources: BTreeMap::new(),
            used: BTreeSet::new(),
        })
    }
    pub fn recall(&self, now: SimTime) -> Option<Vec<Proposal>> {
        (now < self.until).then(|| {
            self.offers
                .iter()
                .filter(|p| !self.used.contains(&p.key))
                .cloned()
                .collect()
        })
    }
    pub fn consume(&mut self, key: &str) {
        if self.offers.iter().any(|p| p.key == key) {
            self.used.insert(key.to_owned());
        }
    }
    /// Retain provider identities only for offers actually present in the shortlist.
    pub fn set_sources(&mut self, sources: BTreeMap<String, String>) {
        self.sources = sources
            .into_iter()
            .filter(|(key, _)| self.offers.iter().any(|offer| &offer.key == key))
            .collect();
    }
    pub fn sources(&self) -> &BTreeMap<String, String> {
        &self.sources
    }
    /// Schedule a bounded refresh for a saved consideration under its owner.
    pub fn arm(&mut self, tx: &mut Transaction<'_>, person: PersonId) -> Result<()> {
        require(
            self.pending.is_none() && self.until > tx.now(),
            "invalid consideration deadline",
        )?;
        self.pending = Some(tx.schedule(
            self.until,
            50,
            crate::Recall {
                person,
                until: self.until,
            },
        )?);
        Ok(())
    }
    pub fn cancel(&self, tx: &mut Transaction<'_>) {
        if let Some(id) = self.pending {
            tx.unschedule(id);
        }
    }
}
