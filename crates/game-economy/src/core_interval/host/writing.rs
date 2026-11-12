//! Personal text pages and physical reading.
use super::*;
use std::sync::Arc;
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CoreWritingResults {
    pub actors: Vec<u64>,
    pub items: Vec<u64>,
}
impl CoreHost {
    fn writing_busy(&self, blocked: &[u64]) -> Vec<u64> {
        let mut busy = self
            .blocked_with_rest(blocked)
            .unwrap_or_else(|| blocked.to_vec());
        busy.par_extend(
            self.core
                .physical
                .work()
                .par_iter()
                .filter(|work| work.phase == game_lot_runtime::Phase::Working)
                .map(|work| work.job.worker),
        );
        busy.par_sort_unstable();
        busy.dedup();
        busy
    }
    fn writing_tick(&self) -> Result<u64, String> {
        let tick = self.core.physical.tick();
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        if offset >= self.core.physical.tick_millis()
            || self.core.next_deadline(None)?.after_millis == 0
        {
            return Err("publish pending interval events before writing".into());
        }
        Ok(tick)
    }
    /// A person reads only an actual page they currently carry and understand.
    pub fn read_texts_autonomously(
        &mut self,
        blocked: &[u64],
    ) -> Result<CoreWritingResults, String> {
        if blocked.par_windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err("invalid occupied people".into());
        }
        let Some(owner) = self.writing.as_ref() else {
            return Ok(CoreWritingResults::default());
        };
        if owner.pages.is_empty() {
            return Ok(CoreWritingResults::default());
        }
        let tick = self.writing_tick()?;
        let busy = self.writing_busy(blocked);
        let reads = owner.auto_reads(&self.core.people, &busy, &self.core.physical);
        if reads.is_empty() {
            return Ok(CoreWritingResults::default());
        }
        let actors = reads.par_iter().map(|row| row.0).collect();
        let items = reads.par_iter().map(|row| row.1).collect();
        let mut next = owner.clone();
        next.record_reads(&reads);
        next.remember_text(&reads, tick);
        let filter = self
            .opinions
            .as_ref()
            .and_then(|opinions| opinions.report_filter());
        let reports: Vec<_> = reads
            .par_iter()
            .filter_map(|&(person, item)| {
                let page = owner.page(item)?;
                if filter
                    .as_ref()
                    .is_some_and(|filter| !filter.accepts(person, page.author))
                {
                    return None;
                }
                let mut claim = page.claim.clone()?;
                claim.learned_from = if page.author == person {
                    claim.learned_from.filter(|&source| source != person)
                } else {
                    Some(page.author)
                };
                let region = self
                    .core
                    .people
                    .binary_search_by_key(&person, |p| p.id)
                    .ok()
                    .and_then(|index| self.core.people[index].region);
                Some((person, claim, region))
            })
            .collect();
        let knowledge = if reports.is_empty() {
            None
        } else {
            let owner = self
                .knowledge
                .as_ref()
                .ok_or("written claims require personal knowledge")?;
            let mut next = game_economy_knowledge::Knowledge {
                max_records: owner.max_records,
                minds: owner.minds.par_iter().cloned().collect(),
                retention: owner.retention.par_iter().cloned().collect(),
            };
            next.receive_reports_at(&reports, tick)
                .map_err(str::to_owned)?;
            Some(next)
        };
        self.writing = Some(next);
        if let Some(knowledge) = knowledge {
            self.knowledge = Some(knowledge);
        }
        Ok(CoreWritingResults { actors, items })
    }
    /// Each author may consume their own paper and ink to create an identified page.
    pub fn write_texts_autonomously(
        &mut self,
        blocked: &[u64],
    ) -> Result<CoreWritingResults, String> {
        if blocked.par_windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err("invalid occupied people".into());
        }
        let Some(owner) = self.writing.as_ref() else {
            return Ok(CoreWritingResults::default());
        };
        if owner.text_authors.is_empty() {
            return Ok(CoreWritingResults::default());
        }
        let tick = self.writing_tick()?;
        let busy = self.writing_busy(blocked);
        let chosen: Vec<_> = owner
            .text_authors
            .par_iter()
            .filter_map(|author| {
                let person = &self.core.people[self
                    .core
                    .people
                    .binary_search_by_key(&author.person, |p| p.id)
                    .ok()?];
                if !author.enabled
                    || author.completed
                    || author.retry_at > tick
                    || person.controlled
                    || person.health == 0
                    || person.region.is_none()
                    || busy.binary_search(&person.id).is_ok()
                    || !owner.can_write(person.id, author.script)
                {
                    return None;
                }
                let template = &owner.text_templates[owner
                    .text_templates
                    .binary_search_by_key(&author.template, |t| t.id)
                    .ok()?];
                Some((person.id, author.script, template.text.clone()))
            })
            .collect();
        if chosen.is_empty() {
            return Ok(CoreWritingResults::default());
        }
        u64::try_from(chosen.len())
            .map_err(|_| "writing action overflow")?
            .checked_add(1)
            .ok_or("writing action overflow")?;
        let start = owner
            .next_item
            .max(self.tool_wear.as_ref().map_or(1, |wear| wear.next_item));
        let items = self
            .core
            .physical
            .next_unused_items(start, chosen.len())
            .ok_or("item identity overflow")?;
        if items.last().is_some_and(|&id| id == u64::MAX) {
            return Err("item identity overflow".into());
        }
        let pages: Vec<_> = chosen
            .par_iter()
            .zip(items.par_iter())
            .enumerate()
            .map(
                |(index, ((person, _, _), &item))| game_lot_runtime::WritingPage {
                    action: index as u64 + 1,
                    writer: *person,
                    paper: owner.paper,
                    paper_quantity: owner.paper_quantity,
                    ink: owner.ink,
                    ink_quantity: owner.ink_quantity,
                    page: owner.page,
                    item,
                },
            )
            .collect();
        let physical = self
            .core
            .physical
            .prepare_interval_writing(&pages, self.core.seed)?;
        let mut outcomes: Vec<_> = physical.transactions().par_iter().copied().collect();
        outcomes.par_sort_unstable_by_key(|row| row.id);
        let accepted: Vec<_> = pages
            .par_iter()
            .zip(chosen.par_iter())
            .filter_map(|(page, (person, script, text))| {
                outcomes
                    .binary_search_by_key(&page.action, |row| row.id)
                    .ok()
                    .filter(|&i| outcomes[i].accepted)
                    .map(|_| crate::WrittenPage {
                        item: page.item,
                        author: *person,
                        script: *script,
                        written_at: tick,
                        claim: None,
                        text: Some(text.clone()),
                    })
            })
            .collect();
        let mut next = owner.clone();
        let mut statuses: Vec<_> = pages
            .par_iter()
            .zip(chosen.par_iter())
            .map(|(page, (person, _, _))| {
                (
                    *person,
                    outcomes
                        .binary_search_by_key(&page.action, |row| row.id)
                        .ok()
                        .is_some_and(|i| outcomes[i].accepted),
                )
            })
            .collect();
        statuses.par_sort_unstable_by_key(|row| row.0);
        let mut authors: Vec<_> = owner.text_authors.par_iter().cloned().collect();
        authors.par_iter_mut().for_each(|author| {
            if let Ok(i) = statuses.binary_search_by_key(&author.person, |row| row.0) {
                author.completed = statuses[i].1;
                if !author.completed {
                    author.retry_at = tick.saturating_add(8);
                }
            }
        });
        next.text_authors = Arc::new(authors);
        if let Some(last) = accepted.last() {
            next.next_item = next.next_item.max(last.item + 1);
        }
        let experiences: Vec<_> = accepted
            .par_iter()
            .map(|page| (page.author, page.item))
            .collect();
        if !accepted.is_empty() {
            let mut pages: Vec<_> = owner.pages.par_iter().cloned().collect();
            pages.par_extend(accepted.par_iter().cloned());
            next.pages = Arc::new(pages);
        }
        next.remember_text(&experiences, tick);
        let mut created: Vec<_> = items.par_iter().copied().collect();
        created.par_sort_unstable();
        let other_updates: Vec<_> = physical
            .item_updates()
            .par_iter()
            .filter(|(id, _)| created.binary_search(id).is_err())
            .copied()
            .collect();
        let tools = self.stage_tools(&[], None, &other_updates)?;
        if !accepted.is_empty() {
            self.core
                .physical
                .validate_interval_consumption(&physical)?;
            self.core.physical.commit_interval_consumption(physical)?;
        }
        if let Some(tools) = tools {
            self.tool_wear = Some(tools);
        }
        let actors = chosen.par_iter().map(|row| row.0).collect();
        let items = accepted.par_iter().map(|page| page.item).collect();
        self.writing = Some(next);
        self.work_checked_revision = None;
        Ok(CoreWritingResults { actors, items })
    }
    /// An author can write a remembered report, including their chosen misstatement.
    pub fn write_claims_autonomously(
        &mut self,
        blocked: &[u64],
    ) -> Result<CoreWritingResults, String> {
        if blocked.par_windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err("invalid occupied people".into());
        }
        let Some(owner) = self.writing.as_ref() else {
            return Ok(CoreWritingResults::default());
        };
        if owner.authors.is_empty() {
            return Ok(CoreWritingResults::default());
        }
        let tick = self.writing_tick()?;
        let busy = self.writing_busy(blocked);
        let knowledge = self
            .knowledge
            .as_ref()
            .ok_or("written claims require personal knowledge")?;
        let recall = knowledge.recall_index();
        let mut minds: Vec<_> = knowledge.minds.par_iter().collect();
        if minds
            .par_windows(2)
            .any(|pair| pair[0].person > pair[1].person)
        {
            minds.par_sort_unstable_by_key(|mind| mind.person);
        }
        let chosen: Vec<_> = owner
            .authors
            .par_iter()
            .filter_map(|author| {
                let person = &self.core.people[self
                    .core
                    .people
                    .binary_search_by_key(&author.person, |p| p.id)
                    .ok()?];
                let mind = minds[minds
                    .binary_search_by_key(&author.person, |row| row.person)
                    .ok()?];
                if author.completed
                    || author.retry_at > tick
                    || person.controlled
                    || person.health == 0
                    || person.region.is_none()
                    || busy.binary_search(&person.id).is_ok()
                    || !owner.can_write(person.id, author.script)
                {
                    return None;
                }
                let mut claim = mind
                    .records
                    .iter()
                    .find(|record| {
                        record.region == author.region
                            && recall.remembers(author.person, record, tick)
                    })?
                    .clone();
                claim.food = (i128::from(claim.food) + i128::from(author.food_delta))
                    .clamp(0, i128::from(u32::MAX)) as u32;
                Some((person.id, author.script, claim))
            })
            .collect();
        if chosen.is_empty() {
            return Ok(CoreWritingResults::default());
        }
        u64::try_from(chosen.len())
            .map_err(|_| "writing action overflow")?
            .checked_add(1)
            .ok_or("writing action overflow")?;
        let start = owner
            .next_item
            .max(self.tool_wear.as_ref().map_or(1, |wear| wear.next_item));
        let items = self
            .core
            .physical
            .next_unused_items(start, chosen.len())
            .ok_or("item identity overflow")?;
        if items.last().is_some_and(|&id| id == u64::MAX) {
            return Err("item identity overflow".into());
        }
        let pages: Vec<_> = chosen
            .par_iter()
            .zip(items.par_iter())
            .enumerate()
            .map(
                |(index, ((person, _, _), &item))| game_lot_runtime::WritingPage {
                    action: index as u64 + 1,
                    writer: *person,
                    paper: owner.paper,
                    paper_quantity: owner.paper_quantity,
                    ink: owner.ink,
                    ink_quantity: owner.ink_quantity,
                    page: owner.page,
                    item,
                },
            )
            .collect();
        let physical = self
            .core
            .physical
            .prepare_interval_writing(&pages, self.core.seed)?;
        let mut outcomes: Vec<_> = physical.transactions().par_iter().copied().collect();
        outcomes.par_sort_unstable_by_key(|row| row.id);
        let accepted: Vec<_> = pages
            .par_iter()
            .zip(chosen.par_iter())
            .filter_map(|(page, (person, script, text))| {
                outcomes
                    .binary_search_by_key(&page.action, |row| row.id)
                    .ok()
                    .filter(|&i| outcomes[i].accepted)
                    .map(|_| crate::WrittenPage {
                        item: page.item,
                        author: *person,
                        script: *script,
                        written_at: tick,
                        claim: Some(text.clone()),
                        text: None,
                    })
            })
            .collect();
        let mut next = owner.clone();
        let mut statuses: Vec<_> = pages
            .par_iter()
            .zip(chosen.par_iter())
            .map(|(page, (person, _, _))| {
                (
                    *person,
                    outcomes
                        .binary_search_by_key(&page.action, |row| row.id)
                        .ok()
                        .is_some_and(|i| outcomes[i].accepted),
                )
            })
            .collect();
        statuses.par_sort_unstable_by_key(|row| row.0);
        let mut authors: Vec<_> = owner.authors.par_iter().cloned().collect();
        authors.par_iter_mut().for_each(|author| {
            if let Ok(i) = statuses.binary_search_by_key(&author.person, |row| row.0) {
                author.completed = statuses[i].1;
                if !author.completed {
                    author.retry_at = tick.saturating_add(8);
                }
            }
        });
        next.authors = Arc::new(authors);
        if let Some(last) = accepted.last() {
            next.next_item = next.next_item.max(last.item + 1);
        }
        if !accepted.is_empty() {
            let mut pages: Vec<_> = owner.pages.par_iter().cloned().collect();
            pages.par_extend(accepted.par_iter().cloned());
            next.pages = Arc::new(pages);
        }
        let mut created: Vec<_> = items.par_iter().copied().collect();
        created.par_sort_unstable();
        let other_updates: Vec<_> = physical
            .item_updates()
            .par_iter()
            .filter(|(id, _)| created.binary_search(id).is_err())
            .copied()
            .collect();
        let tools = self.stage_tools(&[], None, &other_updates)?;
        if !accepted.is_empty() {
            self.core
                .physical
                .validate_interval_consumption(&physical)?;
            self.core.physical.commit_interval_consumption(physical)?;
        }
        if let Some(tools) = tools {
            self.tool_wear = Some(tools);
        }
        let actors = chosen.par_iter().map(|row| row.0).collect();
        let items = accepted.par_iter().map(|page| page.item).collect();
        self.writing = Some(next);
        self.work_checked_revision = None;
        Ok(CoreWritingResults { actors, items })
    }
}
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CoreWritingPracticeResults {
    pub actors: Vec<u64>,
    pub practiced: Vec<u64>,
}
impl CoreHost {
    /// Each learner independently consumes paper and ink to practice a known script.
    pub fn practice_writing_autonomously(
        &mut self,
        blocked: &[u64],
    ) -> Result<CoreWritingPracticeResults, String> {
        if blocked.par_windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err("invalid occupied people".into());
        }
        let Some(owner) = self.writing.as_ref() else {
            return Ok(CoreWritingPracticeResults::default());
        };
        if owner.practice.is_empty() {
            return Ok(CoreWritingPracticeResults::default());
        }
        let tick = self.writing_tick()?;
        let busy = self.writing_busy(blocked);
        let chosen: Vec<_> = owner
            .practice
            .par_iter()
            .filter(|policy| {
                let Ok(index) = self
                    .core
                    .people
                    .binary_search_by_key(&policy.person, |p| p.id)
                else {
                    return false;
                };
                let person = &self.core.people[index];
                policy.enabled
                    && policy.progress < policy.required
                    && policy.retry_at <= tick
                    && !person.controlled
                    && person.health > 0
                    && person.region.is_some()
                    && busy.binary_search(&person.id).is_err()
                    && owner.can_read(person.id, policy.script)
                    && !owner.can_write(person.id, policy.script)
            })
            .map(|policy| (policy.person, policy.script))
            .collect();
        if chosen.is_empty() {
            return Ok(CoreWritingPracticeResults::default());
        }
        u64::try_from(chosen.len())
            .map_err(|_| "practice action overflow")?
            .checked_mul(2)
            .ok_or("practice action overflow")?;
        let actions: Vec<_> = chosen
            .par_iter()
            .enumerate()
            .map(|(index, &(person, _))| {
                let movement = index as u64 * 2 + 1;
                game_lots::Transaction {
                    id: index as u64 + 1,
                    movements: vec![
                        game_lots::Movement {
                            batch: None,
                            id: movement,
                            from: game_lots::Place::Person(person),
                            to: None,
                            good: owner.paper,
                            quantity: owner.paper_quantity,
                            item: None,
                        },
                        game_lots::Movement {
                            batch: None,
                            id: movement + 1,
                            from: game_lots::Place::Person(person),
                            to: None,
                            good: owner.ink,
                            quantity: owner.ink_quantity,
                            item: None,
                        },
                    ],
                    creations: vec![],
                }
            })
            .collect();
        let physical = self
            .core
            .physical
            .prepare_interval_consumption(&actions, self.core.seed)?;
        let mut outcomes: Vec<_> = physical.transactions().par_iter().copied().collect();
        outcomes.par_sort_unstable_by_key(|row| row.id);
        let mut statuses: Vec<_> = chosen
            .par_iter()
            .enumerate()
            .map(|(index, &(person, script))| {
                let accepted = outcomes
                    .binary_search_by_key(&(index as u64 + 1), |row| row.id)
                    .ok()
                    .is_some_and(|i| outcomes[i].accepted);
                (person, script, accepted)
            })
            .collect();
        statuses.par_sort_unstable_by_key(|row| row.0);
        let mut practice: Vec<_> = owner.practice.par_iter().cloned().collect();
        practice.par_iter_mut().for_each(|policy| {
            if let Ok(index) = statuses.binary_search_by_key(&policy.person, |row| row.0) {
                if statuses[index].2 {
                    policy.progress += 1;
                } else {
                    policy.retry_at = tick.saturating_add(8);
                }
            }
        });
        let learned: Vec<_> = practice
            .par_iter()
            .filter(|policy| policy.progress == policy.required)
            .map(|policy| (policy.person, policy.script))
            .collect();
        let mut next = owner.clone();
        next.practice = Arc::new(practice);
        if !learned.is_empty() {
            let mut literacy: Vec<_> = owner.literacy.par_iter().cloned().collect();
            literacy.par_iter_mut().for_each(|row| {
                if learned.binary_search(&(row.person, row.script)).is_ok() {
                    row.write = true;
                }
            });
            next.literacy = Arc::new(literacy);
        }
        let tools = self.stage_tools(&[], None, physical.item_updates())?;
        let succeeded: Vec<_> = statuses
            .par_iter()
            .filter(|row| row.2)
            .map(|row| row.0)
            .collect();
        if !succeeded.is_empty() {
            self.core
                .physical
                .validate_interval_consumption(&physical)?;
            self.core.physical.commit_interval_consumption(physical)?;
        }
        if let Some(tools) = tools {
            self.tool_wear = Some(tools);
        }
        self.writing = Some(next);
        self.work_checked_revision = None;
        Ok(CoreWritingPracticeResults {
            actors: chosen.par_iter().map(|row| row.0).collect(),
            practiced: succeeded,
        })
    }
}
