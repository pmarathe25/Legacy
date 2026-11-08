//! Personal records attached to identified physical pages.
use game_economy_knowledge::Record;
use game_lot_runtime::Snapshot;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;

pub(crate) fn ordered_index<T>(rows: &[T], id: u64, key: impl Fn(&T) -> u64) -> Option<usize> {
    rows.first()
        .and_then(|row| id.checked_sub(key(row)))
        .and_then(|offset| usize::try_from(offset).ok())
        .filter(|&index| rows.get(index).is_some_and(|row| key(row) == id))
        .or_else(|| rows.binary_search_by_key(&id, key).ok())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Writing {
    pub paper: u32,
    pub ink: u32,
    pub page: u32,
    pub paper_quantity: u64,
    pub ink_quantity: u64,
    pub next_item: u64,
    #[serde(default)]
    pub literacy: Arc<Vec<Literacy>>,
    #[serde(default)]
    pub pages: Arc<Vec<Page>>,
    /// Pages this person has already chosen to read, including disregarded claims.
    #[serde(default)]
    pub reads: Arc<Vec<(u64, u64)>>,
    #[serde(default)]
    pub authors: Arc<Vec<Author>>,
    #[serde(default)]
    pub study: Arc<Vec<Study>>,
    #[serde(default)]
    pub practice: Arc<Vec<Practice>>,
    /// Data-defined text a person may choose to write.
    #[serde(default)]
    pub text_templates: Arc<Vec<TextTemplate>>,
    #[serde(default)]
    pub text_authors: Arc<Vec<TextAuthor>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextTemplate {
    pub id: u64,
    pub text: String,
}

/// A personal, revocable choice to write one configured text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextAuthor {
    pub person: u64,
    pub script: u32,
    pub template: u64,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub completed: bool,
    #[serde(default)]
    pub retry_at: u64,
}

/// A person's chosen amount of physical practice toward writing a known script.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Practice {
    pub person: u64,
    pub script: u32,
    pub required: u32,
    #[serde(default)]
    pub progress: u32,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub retry_at: u64,
}

/// A personal decision to learn a script by observing a literate encounter partner.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Study {
    pub person: u64,
    pub script: u32,
    pub required: u32,
    #[serde(default)]
    pub progress: u32,
    #[serde(default)]
    pub enabled: bool,
}

/// A person's independent choice to write one remembered food report.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Author {
    pub person: u64,
    pub script: u32,
    pub region: u64,
    #[serde(default)]
    pub food_delta: i64,
    #[serde(default)]
    pub completed: bool,
    /// Next boundary at which this person will retry after a failed attempt.
    #[serde(default)]
    pub retry_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Literacy {
    pub person: u64,
    pub script: u32,
    #[serde(default)]
    pub read: bool,
    #[serde(default)]
    pub write: bool,
    #[serde(default)]
    pub auto_read: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    pub item: u64,
    pub author: u64,
    pub script: u32,
    pub written_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim: Option<Record>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

impl Writing {
    pub fn normalize_and_validate(
        &mut self,
        people: &HashSet<u64>,
        goods: &HashSet<u32>,
        regions: &HashSet<u64>,
        physical: &Snapshot,
    ) -> Result<(), String> {
        Arc::make_mut(&mut self.literacy).par_sort_unstable_by_key(|row| (row.person, row.script));
        Arc::make_mut(&mut self.pages).par_sort_unstable_by_key(|row| row.item);
        Arc::make_mut(&mut self.reads).par_sort_unstable();
        Arc::make_mut(&mut self.authors).par_sort_unstable_by_key(|row| row.person);
        Arc::make_mut(&mut self.study).par_sort_unstable_by_key(|row| (row.person, row.script));
        Arc::make_mut(&mut self.practice).par_sort_unstable_by_key(|row| row.person);
        Arc::make_mut(&mut self.text_templates).par_sort_unstable_by_key(|row| row.id);
        Arc::make_mut(&mut self.text_authors).par_sort_unstable_by_key(|row| row.person);
        let tick = physical.custody.tick;
        if self.paper == self.ink
            || self.paper == self.page
            || self.ink == self.page
            || !goods.contains(&self.paper)
            || !goods.contains(&self.ink)
            || !goods.contains(&self.page)
            || self.paper_quantity == 0
            || self.ink_quantity == 0
            || self.next_item == 0
            || self
                .literacy
                .par_windows(2)
                .any(|pair| (pair[0].person, pair[0].script) == (pair[1].person, pair[1].script))
            || self
                .literacy
                .par_iter()
                .any(|row| !people.contains(&row.person) || row.script == 0)
            || self
                .pages
                .par_windows(2)
                .any(|pair| pair[0].item == pair[1].item)
            || self.reads.par_windows(2).any(|pair| pair[0] == pair[1])
            || self
                .authors
                .par_windows(2)
                .any(|pair| pair[0].person == pair[1].person)
            || self.authors.par_iter().any(|row| {
                !people.contains(&row.person) || !regions.contains(&row.region) || row.script == 0
            })
            || self
                .study
                .par_windows(2)
                .any(|pair| (pair[0].person, pair[0].script) == (pair[1].person, pair[1].script))
            || self.study.par_iter().any(|row| {
                !people.contains(&row.person)
                    || row.script == 0
                    || row.required == 0
                    || row.progress > row.required
                    || (row.progress == row.required && !self.can_read(row.person, row.script))
            })
            || self
                .practice
                .par_windows(2)
                .any(|pair| pair[0].person == pair[1].person)
            || self.practice.par_iter().any(|row| {
                !people.contains(&row.person)
                    || row.script == 0
                    || row.required == 0
                    || row.progress > row.required
                    || (row.progress == row.required && !self.can_write(row.person, row.script))
            })
            || self.reads.par_iter().any(|&(person, item)| {
                !people.contains(&person) || item == 0 || item >= self.next_item
            })
            || self.pages.par_iter().any(|row| {
                row.item == 0
                    || row.item >= self.next_item
                    || !people.contains(&row.author)
                    || row.script == 0
                    || row.written_at > tick
                    || row.claim.is_some() == row.text.is_some()
                    || row
                        .text
                        .as_ref()
                        .is_some_and(|text| text.is_empty() || text.len() > 4096)
                    || row.claim.as_ref().is_some_and(|claim| {
                        claim.observed_at > row.written_at
                            || !regions.contains(&claim.region)
                            || !people.contains(&claim.witness)
                            || claim.learned_from.is_some_and(|id| !people.contains(&id))
                    })
            })
            || self
                .text_templates
                .par_windows(2)
                .any(|p| p[0].id == p[1].id)
            || self
                .text_templates
                .par_iter()
                .any(|t| t.id == 0 || t.text.is_empty() || t.text.len() > 4096)
            || self
                .text_authors
                .par_windows(2)
                .any(|p| p[0].person == p[1].person)
            || self.text_authors.par_iter().any(|a| {
                !people.contains(&a.person)
                    || a.script == 0
                    || self
                        .text_templates
                        .binary_search_by_key(&a.template, |t| t.id)
                        .is_err()
            })
        {
            return Err("invalid written records".into());
        }
        let mut physical_pages: Vec<_> = physical
            .custody
            .containers
            .par_iter()
            .flat_map_iter(|holder| {
                holder
                    .lots
                    .iter()
                    .filter_map(|lot| lot.item.map(|id| (id, lot.good)))
            })
            .collect();
        physical_pages.par_sort_unstable_by_key(|row| row.0);
        if physical_pages.par_iter().any(|&(id, good)| {
            good == self.page
                && (id >= self.next_item
                    || self
                        .pages
                        .binary_search_by_key(&id, |row| row.item)
                        .is_err())
        }) || self.pages.par_iter().any(|page| {
            physical_pages
                .binary_search_by_key(&page.item, |row| row.0)
                .ok()
                .is_none_or(|i| physical_pages[i].1 != self.page)
        }) {
            return Err("written pages do not match physical custody".into());
        }
        Ok(())
    }

    pub fn can_read(&self, person: u64, script: u32) -> bool {
        self.literacy
            .binary_search_by_key(&(person, script), |row| (row.person, row.script))
            .ok()
            .is_some_and(|i| self.literacy[i].read)
    }

    /// Change a person's authored literacy without mutating an earlier generation.
    pub fn literacy_mut(&mut self) -> &mut Vec<Literacy> {
        Arc::make_mut(&mut self.literacy)
    }

    pub fn can_write(&self, person: u64, script: u32) -> bool {
        self.literacy
            .binary_search_by_key(&(person, script), |row| (row.person, row.script))
            .ok()
            .is_some_and(|i| self.literacy[i].write)
    }

    pub fn page(&self, id: u64) -> Option<&Page> {
        ordered_index(&self.pages, id, |row| row.item).map(|i| &self.pages[i])
    }
    pub fn add_page(&mut self, page: Page) {
        Arc::make_mut(&mut self.pages).push(page);
    }

    /// Observe an actual page during an encounter; no teacher's consent or
    /// automatic acceptance of the page's claim is implied.
    pub fn advance_study(
        &mut self,
        opening: &Writing,
        physical: &game_lot_runtime::Runtime,
        updates: &[(u64, Option<game_lots::Place>)],
        events: &[game_social::Encounter],
        tick: u64,
    ) {
        if opening.study.is_empty() || events.is_empty() || opening.pages.is_empty() {
            return;
        }
        let mut pages: Vec<_> = opening
            .pages
            .par_iter()
            .filter_map(|page| {
                let place = physical.item_place(page.item)?;
                let game_lots::Place::Person(holder) = place else {
                    return None;
                };
                if updates
                    .binary_search_by_key(&page.item, |row| row.0)
                    .ok()
                    .is_some_and(|i| updates[i].1 != Some(place))
                {
                    return None;
                }
                Some((holder, page.script))
            })
            .collect();
        pages.par_sort_unstable();
        let mut partners: Vec<_> = events
            .par_iter()
            .filter(|event| event.tick == tick)
            .flat_map_iter(|event| [(event.a, event.b), (event.b, event.a)])
            .collect();
        partners.par_sort_unstable();
        let eligible: Vec<_> = opening
            .study
            .par_iter()
            .enumerate()
            .filter_map(|(index, study)| {
                if !study.enabled
                    || study.progress >= study.required
                    || opening.can_read(study.person, study.script)
                {
                    return None;
                }
                let start = partners.partition_point(|row| row.0 < study.person);
                partners[start..]
                    .iter()
                    .take_while(|row| row.0 == study.person)
                    .any(|&(_, other)| {
                        opening.can_read(other, study.script)
                            && (pages.binary_search(&(study.person, study.script)).is_ok()
                                || pages.binary_search(&(other, study.script)).is_ok())
                    })
                    .then_some(index)
            })
            .collect();
        if eligible.is_empty() {
            return;
        }
        let selected: Vec<_> = eligible
            .par_chunk_by(|a, b| opening.study[*a].person == opening.study[*b].person)
            .map(|group| group[0])
            .collect();
        let studies = Arc::make_mut(&mut self.study);
        studies
            .par_iter_mut()
            .enumerate()
            .for_each(|(index, study)| {
                if selected.binary_search(&index).is_ok() {
                    study.progress += 1;
                }
            });
        let learned: Vec<_> = selected
            .par_iter()
            .filter_map(|&index| {
                let study = &studies[index];
                (study.progress == study.required).then_some((study.person, study.script))
            })
            .collect();
        if learned.is_empty() {
            return;
        }
        let literacy = Arc::make_mut(&mut self.literacy);
        literacy.par_iter_mut().for_each(|row| {
            if learned.binary_search(&(row.person, row.script)).is_ok() {
                row.read = true;
            }
        });
        let missing: Vec<_> = learned
            .par_iter()
            .filter(|key| {
                literacy
                    .binary_search_by_key(*key, |row| (row.person, row.script))
                    .is_err()
            })
            .map(|&(person, script)| Literacy {
                person,
                script,
                read: true,
                write: false,
                auto_read: false,
            })
            .collect();
        literacy.par_extend(missing);
        literacy.par_sort_unstable_by_key(|row| (row.person, row.script));
    }

    /// Independent people choose a held, unread page; no other person's knowledge is inspected.
    pub fn auto_reads(
        &self,
        people: &[crate::Person],
        busy: &[u64],
        physical: &game_lot_runtime::Runtime,
    ) -> Vec<(u64, u64)> {
        if self.pages.is_empty()
            || !self
                .literacy
                .par_iter()
                .any(|row| row.read && row.auto_read)
        {
            return Vec::new();
        }
        let mut candidates: Vec<_> = self
            .pages
            .par_iter()
            .filter_map(|page| {
                let game_lots::Place::Person(person) = physical.item_place(page.item)? else {
                    return None;
                };
                let actor = &people[ordered_index(people, person, |row| row.id)?];
                let key = (person, page.script);
                let direct = self
                    .literacy
                    .first()
                    .and_then(|row| person.checked_sub(row.person))
                    .and_then(|offset| usize::try_from(offset).ok())
                    .filter(|&i| {
                        self.literacy
                            .get(i)
                            .is_some_and(|row| (row.person, row.script) == key)
                    });
                let policy = &self.literacy[direct.or_else(|| {
                    self.literacy
                        .binary_search_by_key(&key, |row| (row.person, row.script))
                        .ok()
                })?];
                (!actor.controlled
                    && actor.health > 0
                    && actor.region.is_some()
                    && policy.read
                    && policy.auto_read
                    && busy.binary_search(&person).is_err()
                    && self.reads.binary_search(&(person, page.item)).is_err())
                .then_some((person, page.item))
            })
            .collect();
        candidates.par_sort_unstable();
        candidates
            .par_chunk_by(|a, b| a.0 == b.0)
            .map(|group| group[0])
            .collect()
    }

    pub fn record_reads(&mut self, rows: &[(u64, u64)]) {
        if rows.is_empty() {
            return;
        }
        let mut reads: Vec<_> = self
            .reads
            .par_iter()
            .copied()
            .chain(rows.par_iter().copied())
            .collect();
        reads.par_sort_unstable();
        self.reads = Arc::new(
            reads
                .par_chunk_by(|a, b| a == b)
                .map(|group| group[0])
                .collect(),
        );
    }

    /// Forget contents only when the corresponding physical item was destroyed.
    pub fn retire(&mut self, updates: &[(u64, Option<game_lots::Place>)]) {
        if updates.par_iter().any(|row| row.1.is_none()) {
            self.pages = Arc::new(
                self.pages
                    .par_iter()
                    .filter(|page| {
                        updates
                            .binary_search_by_key(&page.item, |row| row.0)
                            .ok()
                            .is_none_or(|i| updates[i].1.is_some())
                    })
                    .cloned()
                    .collect(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ordered_index;

    #[test]
    fn ordered_lookup_handles_gaps_and_nonzero_first_identity() {
        let rows = [10, 12, 13];
        assert_eq!(ordered_index(&rows, 10, |row| *row), Some(0));
        assert_eq!(ordered_index(&rows, 11, |row| *row), None);
        assert_eq!(ordered_index(&rows, 12, |row| *row), Some(1));
        assert_eq!(ordered_index(&rows, 13, |row| *row), Some(2));
    }
}
