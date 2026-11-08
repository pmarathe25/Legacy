//! Personal retention of text actually written or read.
use crate::writing::Writing;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, sync::Arc};

/// An individual's choice to retain a bounded number of written texts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextRetention {
    pub person: u64,
    pub capacity: usize,
    #[serde(default)]
    pub enabled: bool,
}

/// An individual's recollection, independent of the physical page's survival.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextMemory {
    pub person: u64,
    pub item: u64,
    pub text: Arc<str>,
    pub first_read_at: u64,
    pub read_at: u64,
}

impl Writing {
    /// Retain only texts actually experienced at this boundary. Capacity and
    /// future willingness belong to the individual, and a repeated reading
    /// refreshes that person's existing recollection.
    pub(crate) fn remember_text(&mut self, experiences: &[(u64, u64)], tick: u64) {
        if experiences.is_empty() || self.text_retention.is_empty() {
            return;
        }
        let incoming: Vec<_> = experiences
            .par_iter()
            .filter_map(|&(person, item)| {
                let policy = &self.text_retention[self
                    .text_retention
                    .binary_search_by_key(&person, |p| p.person)
                    .ok()?];
                if !policy.enabled {
                    return None;
                }
                let page = self.page(item)?;
                let first_read_at = self
                    .text_memories
                    .binary_search_by_key(&(person, item), |m| (m.person, m.item))
                    .ok()
                    .map_or(tick, |i| self.text_memories[i].first_read_at);
                Some(TextMemory {
                    person,
                    item,
                    text: Arc::from(page.text.as_deref()?),
                    first_read_at,
                    read_at: tick,
                })
            })
            .collect();
        if incoming.is_empty() {
            return;
        }
        let mut rows: Vec<_> = self
            .text_memories
            .par_iter()
            .cloned()
            .chain(incoming.into_par_iter())
            .collect();
        rows.par_sort_unstable_by_key(|m| {
            (
                m.person,
                std::cmp::Reverse(m.read_at),
                std::cmp::Reverse(m.item),
            )
        });
        let mut kept: Vec<_> = rows
            .par_chunk_by(|a, b| a.person == b.person)
            .flat_map_iter(|group| {
                let capacity = self
                    .text_retention
                    .binary_search_by_key(&group[0].person, |p| p.person)
                    .ok()
                    .map_or(1024, |i| self.text_retention[i].capacity);
                let mut items =
                    (capacity > 1).then(|| HashSet::with_capacity(capacity.min(group.len())));
                group
                    .iter()
                    .enumerate()
                    .filter(move |(index, memory)| {
                        if let Some(items) = &mut items {
                            items.len() < capacity && items.insert(memory.item)
                        } else {
                            *index == 0
                        }
                    })
                    .map(|(_, memory)| memory.clone())
            })
            .collect();
        kept.par_sort_unstable_by_key(|m| (m.person, m.item));
        self.text_memories = Arc::new(kept);
    }
}
