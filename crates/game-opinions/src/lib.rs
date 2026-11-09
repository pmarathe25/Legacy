//! Directed personal trust, updated by directly experienced promises.
use game_agreements::{Contract, Event as AgreementEvent, EventKind};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
mod confinement;
mod harm;
mod opposed_action;
mod personal_change;
mod report_trust;
mod social_merge;
mod teaching;
pub use confinement::{
    Experience as ConfinementExperience, ExperienceKind as ConfinementExperienceKind,
};
pub use opposed_action::{Experience as OpposedActionExperience, Outcome as OpposedActionOutcome};
pub use personal_change::PersonalChange;
pub use teaching::{Experience as TeachingExperience, Role as TeachingRole};
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Opinions {
    pub rules: Rules,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sensitivities: Arc<Vec<Sensitivity>>,
    #[serde(default)]
    pub records: Vec<Opinion>,
    #[serde(default)]
    pub events: Vec<Event>,
}
impl Clone for Opinions {
    fn clone(&self) -> Self {
        Self {
            rules: self.rules.clone(),
            sensitivities: Arc::clone(&self.sensitivities),
            records: self.records.par_iter().cloned().collect(),
            events: self.events.par_iter().cloned().collect(),
        }
    }
}
pub use game_opinions_model::{
    AffinityRules, ConfinementReaction, Confrontation, Event, ObligationDevelopment, Opinion,
    OpposedActionReaction, RelationshipCase, ReportReception, Rules, Sensitivity,
    SubjectiveReaction, TeachingReaction,
};

pub struct Index<'a> {
    rules: &'a Rules,
    records: IndexRecords<'a>,
}
enum IndexRecords<'a> {
    Ordered(&'a [Opinion]),
    Indexed(Vec<&'a Opinion>),
}
/// Independent receiver policies over committed directed trust.
pub struct ReportFilter<'a> {
    trust: Index<'a>,
    policies: Vec<(u64, ReportReception)>,
    dense_base: Option<u64>,
}
impl ReportFilter<'_> {
    pub fn accepts(&self, receiver: u64, sender: u64) -> bool {
        let policy = if let Some(first) = self.dense_base {
            receiver
                .checked_sub(first)
                .and_then(|i| usize::try_from(i).ok())
                .and_then(|i| self.policies.get(i))
                .map(|p| p.1)
        } else {
            self.policies
                .binary_search_by_key(&receiver, |p| p.0)
                .ok()
                .map(|i| self.policies[i].1)
        };
        match policy {
            Some(ReportReception::Always) => true,
            Some(ReportReception::Never) => false,
            Some(ReportReception::TrustAtLeast(threshold)) => {
                self.trust.trust(receiver, sender) >= threshold
            }
            None => self
                .trust
                .rules
                .minimum_report_trust
                .is_none_or(|threshold| self.trust.trust(receiver, sender) >= threshold),
        }
    }
}
impl Index<'_> {
    fn get(&self, observer: u64, target: u64) -> Option<&Opinion> {
        let key = (observer, target);
        match &self.records {
            IndexRecords::Ordered(rows) => rows
                .binary_search_by_key(&key, |row| (row.observer, row.target))
                .ok()
                .map(|i| &rows[i]),
            IndexRecords::Indexed(rows) => rows
                .binary_search_by_key(&key, |row| (row.observer, row.target))
                .ok()
                .map(|i| rows[i]),
        }
    }
    pub fn assessment(&self, observer: u64, target: u64) -> (i32, u64) {
        self.get(observer, target)
            .map_or((self.rules.initial, 0), |row| (row.trust, row.fear))
    }
    pub fn feelings(&self, observer: u64, target: u64) -> (u64, u64) {
        self.get(observer, target)
            .map_or((0, 0), |row| (row.fear, row.obligation))
    }
    pub fn fear(&self, observer: u64, target: u64) -> u64 {
        self.feelings(observer, target).0
    }
    pub fn obligation(&self, observer: u64, target: u64) -> u64 {
        self.feelings(observer, target).1
    }
    pub fn affinity(&self, observer: u64, target: u64) -> i32 {
        self.get(observer, target).map_or(0, |row| row.affinity)
    }
    pub fn trust(&self, observer: u64, target: u64) -> i32 {
        self.get(observer, target)
            .map_or(self.rules.initial, |row| row.trust)
    }
}
impl Opinions {
    pub fn tracks_social_affinity_weights(&self) -> bool {
        self.sensitivities.par_iter().any(|row| {
            row.social_affinity_weight.is_some()
                || row
                    .relationship_cases
                    .iter()
                    .any(|case| case.social_affinity_weight.is_some())
        })
    }
    pub fn tracks_teaching_reactions(&self) -> bool {
        self.sensitivities.par_iter().any(|row| {
            row.teaching_reaction.is_some()
                || row
                    .relationship_cases
                    .iter()
                    .any(|case| case.teaching_reaction.is_some())
        })
    }
    /// Apply the observer's own signed interpretation of an actual lesson.
    pub fn learn_teaching(&mut self, experiences: &[TeachingExperience], tick: u64) {
        teaching::apply(self, experiences, tick);
    }
    pub fn tracks_opposed_action_reactions(&self) -> bool {
        self.sensitivities.par_iter().any(|row| {
            row.opposed_action_reaction.is_some()
                || row
                    .relationship_cases
                    .iter()
                    .any(|case| case.opposed_action_reaction.is_some())
        })
    }
    /// Apply each observer's own interpretation of a directly opposed action.
    pub fn learn_opposed_actions(&mut self, experiences: &[OpposedActionExperience], tick: u64) {
        opposed_action::apply(self, experiences, tick);
    }
    pub fn tracks_confinement_reactions(&self) -> bool {
        self.sensitivities.par_iter().any(|row| {
            row.confinement_reaction.is_some()
                || row
                    .relationship_cases
                    .iter()
                    .any(|case| case.confinement_reaction.is_some())
        })
    }
    /// Interpret only capture and release experiences reported by their observer.
    pub fn learn_confinement(&mut self, experiences: &[ConfinementExperience], tick: u64) {
        confinement::apply(self, experiences, tick);
    }
    /// Apply simultaneous signed interpretations supplied by their observers.
    pub fn apply_personal_changes(&mut self, changes: &[PersonalChange], tick: u64) {
        personal_change::apply(self, changes, tick);
    }
    /// Detach a personal disposition generation only when it is actually changed.
    /// Parallel indexed cloning preserves canonical order for large populations.
    pub fn sensitivities_mut(&mut self) -> &mut Vec<Sensitivity> {
        if Arc::get_mut(&mut self.sensitivities).is_none() {
            self.sensitivities = Arc::new(self.sensitivities.par_iter().cloned().collect());
        }
        Arc::get_mut(&mut self.sensitivities).expect("exclusive sensitivity generation")
    }
    /// Time updates each person's existing fear independently of trust and obligations.
    pub fn decay_fear(&mut self) {
        let decay = self.rules.fear_decay_per_tick;
        if decay > 0 && self.records.par_iter().any(|row| row.fear > 0) {
            self.records
                .par_iter_mut()
                .for_each(|row| row.fear = row.fear.saturating_sub(decay));
        }
    }
    /// Apply a simultaneous set of directly witnessed takings by directed pair.
    pub fn witness_thefts(&mut self, incidents: &[(u64, u64)], tick: u64) {
        harm::theft(self, incidents, tick);
        if !incidents.is_empty() && harm::tracks_affinity(self, harm::AffinityKind::StockUse) {
            let experiences: Vec<_> = incidents
                .par_iter()
                .map(|&(observer, actor)| (observer, actor, 1))
                .collect();
            harm::affinity(self, &experiences, harm::AffinityKind::StockUse);
        }
    }
    pub fn tracks_affinity_help(&self) -> bool {
        harm::tracks_affinity(self, harm::AffinityKind::Help)
    }
    pub fn tracks_affinity_encounters(&self) -> bool {
        harm::tracks_affinity(self, harm::AffinityKind::Encounter)
            || harm::tracks_affinity(self, harm::AffinityKind::PressuredEncounter)
    }
    pub fn tracks_site_affinity(&self) -> bool {
        harm::tracks_affinity(self, harm::AffinityKind::SiteUse)
            || harm::tracks_affinity(self, harm::AffinityKind::SiteDestruction)
    }
    pub fn tracks_field_affinity(&self) -> bool {
        harm::tracks_affinity(self, harm::AffinityKind::FieldUse)
    }
    /// Apply only observers' signed interpretations of witnessed field use.
    pub fn learn_field_affinity(&mut self, events: &[(u64, u64)]) {
        if events.is_empty() || !self.tracks_field_affinity() {
            return;
        }
        let rows: Vec<_> = events
            .par_iter()
            .map(|&(observer, actor)| (observer, actor, 1))
            .collect();
        harm::affinity(self, &rows, harm::AffinityKind::FieldUse);
    }
    pub fn tracks_salvage_affinity(&self) -> bool {
        harm::tracks_affinity(self, harm::AffinityKind::SiteSalvage)
    }
    pub fn tracks_work_affinity(&self) -> bool {
        harm::tracks_affinity(self, harm::AffinityKind::WorkSuccess)
            || harm::tracks_affinity(self, harm::AffinityKind::WorkRefusal)
            || harm::tracks_affinity(self, harm::AffinityKind::WorkFailure)
    }
    pub fn tracks_report_discrepancy(&self) -> bool {
        harm::tracks_affinity(self, harm::AffinityKind::ReportDiscrepancy)
            || report_trust::enabled(self)
    }
    /// Reactions to observed differences, not a judgment that a report was dishonest.
    pub fn learn_report_discrepancies(&mut self, observations: &[(u64, u64)], tick: u64) {
        if observations.is_empty() || !self.tracks_report_discrepancy() {
            return;
        }
        let events: Vec<_> = observations
            .par_iter()
            .map(|&(observer, speaker)| (observer, speaker, 1))
            .collect();
        harm::affinity(self, &events, harm::AffinityKind::ReportDiscrepancy);
        report_trust::apply(self, observations, tick);
    }
    /// Interpret only personally witnessed outcomes, without creating obligations.
    pub fn learn_work_affinity(&mut self, events: &[(u64, u64, bool)]) {
        if events.is_empty() || !self.tracks_work_affinity() {
            return;
        }
        let success: Vec<_> = events
            .par_iter()
            .filter(|row| row.2)
            .map(|row| (row.0, row.1, 1))
            .collect();
        let refusals: Vec<_> = events
            .par_iter()
            .filter(|row| !row.2)
            .map(|row| (row.0, row.1, 1))
            .collect();
        harm::affinity(self, &success, harm::AffinityKind::WorkSuccess);
        harm::affinity(self, &refusals, harm::AffinityKind::WorkRefusal);
    }
    /// Interpret witnessed failed attempts separately from refusals.
    pub fn learn_work_failure_affinity(&mut self, events: &[(u64, u64)]) {
        if events.is_empty() || !harm::tracks_affinity(self, harm::AffinityKind::WorkFailure) {
            return;
        }
        let rows: Vec<_> = events
            .par_iter()
            .map(|&(observer, worker)| (observer, worker, 1))
            .collect();
        harm::affinity(self, &rows, harm::AffinityKind::WorkFailure);
    }
    /// Each observer decides how to interpret witnessed recovery of their claimed debris.
    pub fn learn_salvage_affinity(&mut self, events: &[(u64, u64)]) {
        if !events.is_empty() && self.tracks_salvage_affinity() {
            let events: Vec<_> = events
                .par_iter()
                .map(|&(observer, actor)| (observer, actor, 1))
                .collect();
            harm::affinity(self, &events, harm::AffinityKind::SiteSalvage);
        }
    }
    /// Update only from each observer's signed interpretation of witnessed site actions.
    pub fn learn_site_affinity(&mut self, events: &[(u64, u64, bool)]) {
        if events.is_empty() || !self.tracks_site_affinity() {
            return;
        }
        let use_events: Vec<_> = events
            .par_iter()
            .filter(|e| !e.2)
            .map(|e| (e.0, e.1, 1))
            .collect();
        let destruction: Vec<_> = events
            .par_iter()
            .filter(|e| e.2)
            .map(|e| (e.0, e.1, 1))
            .collect();
        harm::affinity(self, &use_events, harm::AffinityKind::SiteUse);
        harm::affinity(self, &destruction, harm::AffinityKind::SiteDestruction);
    }
    pub fn tracks_harm(&self) -> bool {
        self.rules.fear_per_damage > 0
            || harm::tracks_affinity(self, harm::AffinityKind::Harm)
            || self
                .sensitivities
                .par_iter()
                .any(|row| row.fear_per_damage.is_some_and(|value| value > 0))
    }
    pub fn tracks_help(&self) -> bool {
        self.rules.gratitude_gain > 0
            || self.tracks_affinity_help()
            || self.rules.obligation_relief > 0
            || self.sensitivities.par_iter().any(|row| {
                row.gratitude_gain.is_some_and(|value| value > 0)
                    || row.obligation_relief.is_some_and(|value| value > 0)
                    || row.relationship_cases.iter().any(|case| {
                        case.obligation_gain.is_some_and(|n| n > 0)
                            || case.obligation_relief.is_some_and(|n| n > 0)
                    })
            })
    }
    pub fn tracks_promise_obligation(&self) -> bool {
        self.sensitivities.par_iter().any(|person| {
            person.promise_obligation_gain.is_some_and(|n| n > 0)
                || person
                    .relationship_cases
                    .iter()
                    .any(|case| case.promise_obligation_gain.is_some_and(|n| n > 0))
        })
    }
    /// A promise changes feelings only where the maker's own disposition says it does.
    pub fn learn_promises(&mut self, promises: &[(u64, u64)]) {
        if !promises.is_empty() && self.tracks_promise_obligation() {
            harm::promises(self, promises);
        }
    }
    /// Attach only an authored developmental disposition to actual newborns.
    pub fn enroll_newborn_development(&mut self, newborns: &[u64]) {
        let Some(rule) = self.rules.newborn_obligation_development.as_ref() else {
            return;
        };
        if newborns.is_empty() {
            return;
        }
        let mut existing: Vec<_> = self.sensitivities.par_iter().map(|p| p.person).collect();
        existing.par_sort_unstable();
        let mut newborns = newborns.to_vec();
        newborns.par_sort_unstable();
        let added: Vec<_> = newborns
            .par_chunk_by(|a, b| a == b)
            .map(|group| group[0])
            .filter(|id| existing.binary_search(id).is_err())
            .map(|person| Sensitivity {
                person,
                report_reception: None,
                affinity_report_discrepancy: None,
                trust_report_discrepancy: None,
                trust_promise_fulfilled: None,
                trust_promise_unfulfilled: None,
                fear_per_damage: None,
                gratitude_gain: None,
                promise_obligation_gain: None,
                obligation_relief: None,
                theft_loss: None,
                trust_stock_use: None,
                confrontation: None,
                affinity_help: None,
                affinity_harm_per_damage: None,
                affinity_encounter: None,
                affinity_pressured_encounter: None,
                social_affinity_weight: None,
                affinity_stock_use: None,
                affinity_site_use: None,
                affinity_field_use: None,
                affinity_site_destruction: None,
                affinity_site_salvage: None,
                affinity_work_success: None,
                affinity_work_refusal: None,
                affinity_work_failure: None,
                opposed_action_reaction: None,
                teaching_reaction: None,
                confinement_reaction: None,
                relationship_cases: vec![],
                obligation_development: Some(rule.clone()),
            })
            .collect();
        if !added.is_empty() {
            self.sensitivities_mut().par_extend(added);
        }
    }
    pub fn tracks_obligation_development(&self) -> bool {
        self.sensitivities
            .par_iter()
            .any(|p| p.obligation_development.is_some())
    }
    /// Each configured child interprets actual caregiving or absence according to their own disposition.
    pub fn develop_obligation(&mut self, experiences: &[(u64, u64, bool)]) {
        if experiences.is_empty() || !self.tracks_obligation_development() {
            return;
        }
        let mut experiences = experiences.to_vec();
        if experiences.par_windows(2).any(|rows| rows[0].0 > rows[1].0) {
            experiences.par_sort_unstable_by_key(|e| e.0);
        }
        let dense = experiences
            .first()
            .zip(experiences.last())
            .and_then(|(first, last)| {
                (last.0.checked_sub(first.0) == Some((experiences.len() - 1) as u64)
                    && experiences
                        .par_windows(2)
                        .all(|rows| rows[0].0.checked_add(1) == Some(rows[1].0)))
                .then_some(first.0)
            });
        let default = self.rules.gratitude_gain;
        let find_experience = |person: u64| {
            dense
                .and_then(|first| person.checked_sub(first))
                .and_then(|offset| usize::try_from(offset).ok())
                .filter(|&i| i < experiences.len())
                .or_else(|| {
                    dense
                        .is_none()
                        .then(|| experiences.binary_search_by_key(&person, |e| e.0).ok())
                        .flatten()
                })
        };
        let changes = self.sensitivities.par_iter().any(|person| {
            let Some(rule) = &person.obligation_development else {
                return false;
            };
            let Some(index) = find_experience(person.person) else {
                return false;
            };
            let (_, age, cared) = experiences[index];
            if age >= rule.until_age {
                return false;
            }
            let delta = if cared {
                rule.cared_change
            } else {
                rule.unattended_change
            };
            let next = (i128::from(person.gratitude_gain.unwrap_or(default)) + i128::from(delta))
                .clamp(i128::from(rule.minimum), i128::from(rule.maximum))
                as u64;
            if person.gratitude_gain != Some(next) {
                return true;
            }
            let promise_delta = if cared {
                rule.cared_promise_change
            } else {
                rule.unattended_promise_change
            };
            promise_delta.is_some_and(|delta| {
                let next = (i128::from(person.promise_obligation_gain.unwrap_or(0))
                    + i128::from(delta))
                .clamp(i128::from(rule.minimum), i128::from(rule.maximum))
                    as u64;
                person.promise_obligation_gain != Some(next)
            })
        });
        if !changes {
            return;
        }
        self.sensitivities_mut().par_iter_mut().for_each(|person| {
            let Some(rule) = &person.obligation_development else {
                return;
            };
            let Some(index) = find_experience(person.person) else {
                return;
            };
            let (_, age, cared) = experiences[index];
            if age >= rule.until_age {
                return;
            }
            let delta = if cared {
                rule.cared_change
            } else {
                rule.unattended_change
            };
            let current = i128::from(person.gratitude_gain.unwrap_or(default));
            person.gratitude_gain = Some(
                (current + i128::from(delta))
                    .clamp(i128::from(rule.minimum), i128::from(rule.maximum))
                    as u64,
            );
            let promise_delta = if cared {
                rule.cared_promise_change
            } else {
                rule.unattended_promise_change
            };
            if let Some(delta) = promise_delta {
                let current = i128::from(person.promise_obligation_gain.unwrap_or(0));
                person.promise_obligation_gain = Some(
                    (current + i128::from(delta))
                        .clamp(i128::from(rule.minimum), i128::from(rule.maximum))
                        as u64,
                );
            }
        });
    }
    pub fn reset_for_generation(&mut self) {
        self.records.clear();
        self.events.clear();
        let old = std::mem::take(&mut self.sensitivities);
        if let Ok(rows) = Arc::try_unwrap(old) {
            rows.into_par_iter().for_each(drop);
        }
    }
    pub fn learn_harm(&mut self, harm: &[(u64, u64, u32)]) {
        harm::apply(self, harm);
        if harm::tracks_affinity(self, harm::AffinityKind::Harm) {
            harm::affinity(
                self,
                &harm
                    .par_iter()
                    .map(|&(a, b, n)| (a, b, u64::from(n)))
                    .collect::<Vec<_>>(),
                harm::AffinityKind::Harm,
            );
        }
    }
    pub fn learn_help(&mut self, help: &[(u64, u64)]) {
        harm::help(self, help);
        self.learn_affinity_help(help);
    }
    /// A personal interpretation of receiving a gift, whether or not it relieved hunger.
    pub fn learn_affinity_help(&mut self, help: &[(u64, u64)]) {
        if self.tracks_affinity_help() {
            harm::affinity(
                self,
                &help.par_iter().map(|&(a, b)| (a, b, 1)).collect::<Vec<_>>(),
                harm::AffinityKind::Help,
            );
        }
    }
    pub fn validate(&self, people: &HashSet<u64>, now: u64) -> Result<(), &'static str> {
        let r = &self.rules;
        if r.newborn_obligation_development
            .as_ref()
            .is_some_and(|rule| rule.until_age == 0 || rule.minimum > rule.maximum)
            || r.affinity.as_ref().is_some_and(|a| a.limit <= 0)
            || r.minimum > r.maximum
            || r.initial < r.minimum
            || r.initial > r.maximum
            || r.minimum_report_trust
                .is_some_and(|threshold| threshold < r.minimum || threshold > r.maximum)
        {
            return Err("invalid trust rules");
        }
        let mut ids: Vec<_> = self
            .records
            .par_iter()
            .map(|row| (row.observer, row.target))
            .collect();
        ids.par_sort_unstable();
        let mut sensitivities: Vec<_> = self
            .sensitivities
            .par_iter()
            .map(|row| row.person)
            .collect();
        sensitivities.par_sort_unstable();
        if sensitivities.par_windows(2).any(|row| row[0] == row[1])
            || sensitivities
                .par_iter()
                .any(|person| !people.contains(person))
            || ids.par_windows(2).any(|p| p[0] == p[1])
            || self.sensitivities.par_iter().any(|row| {
                let mut targets = HashSet::new();
                row.relationship_cases.iter().any(|case| {
                    case.target == row.person
                        || !people.contains(&case.target)
                        || !targets.insert(case.target)
                }) || row
                    .obligation_development
                    .as_ref()
                    .is_some_and(|rule| rule.until_age == 0 || rule.minimum > rule.maximum)
                    || row.confrontation.as_ref().is_some_and(|policy| {
                        policy.interval == 0
                            || policy.trust_at_most < r.minimum
                            || policy.trust_at_most > r.maximum
                    })
            })
            || self.records.par_iter().any(|row| {
                row.observer == row.target
                    || !people.contains(&row.observer)
                    || !people.contains(&row.target)
                    || row.trust < r.minimum
                    || row.trust > r.maximum
                    || r.affinity
                        .as_ref()
                        .is_some_and(|a| row.affinity < -a.limit || row.affinity > a.limit)
            })
            || self.events.par_iter().any(|event| {
                event.tick > now
                    || event.observer == event.target
                    || !people.contains(&event.observer)
                    || !people.contains(&event.target)
                    || event.before < r.minimum
                    || event.before > r.maximum
                    || event.after < r.minimum
                    || event.after > r.maximum
            })
        {
            return Err("invalid trust state");
        }
        Ok(())
    }
    /// Record a directly witnessed violation without changing anyone else's opinion.
    pub fn witness_theft(&mut self, observer: u64, target: u64, tick: u64) {
        self.witness_thefts(&[(observer, target)], tick);
    }
    pub fn index(&self) -> Index<'_> {
        let records = if self
            .records
            .par_windows(2)
            .any(|p| (p[0].observer, p[0].target) > (p[1].observer, p[1].target))
        {
            let mut records: Vec<_> = self.records.par_iter().collect();
            records.par_sort_unstable_by_key(|r| (r.observer, r.target));
            IndexRecords::Indexed(records)
        } else {
            IndexRecords::Ordered(&self.records)
        };
        Index {
            rules: &self.rules,
            records,
        }
    }
    /// Build only when some receiver filters reports.
    pub fn report_filter(&self) -> Option<ReportFilter<'_>> {
        let mut policies: Vec<_> = self
            .sensitivities
            .par_iter()
            .filter_map(|s| s.report_reception.map(|policy| (s.person, policy)))
            .collect();
        if policies.is_empty() && self.rules.minimum_report_trust.is_none() {
            return None;
        }
        if policies.par_windows(2).any(|p| p[0].0 > p[1].0) {
            policies.par_sort_unstable_by_key(|p| p.0);
        }
        let dense_base = policies
            .first()
            .zip(policies.last())
            .and_then(|(first, last)| {
                (last.0.checked_sub(first.0) == Some((policies.len() - 1) as u64))
                    .then_some(first.0)
            });
        Some(ReportFilter {
            trust: self.index(),
            policies,
            dense_base,
        })
    }
    /// The host validates identities and contract/event links on import; all effects
    /// in this boundary are combined before the bounded trust update.
    pub fn advance_validated(
        &mut self,
        contracts: &[Contract],
        events: &[AgreementEvent],
        tick: u64,
    ) {
        self.advance_with_social(contracts, events, &[], tick);
    }
    /// Combine agreement outcomes and encounters without rewarding pressured participants.
    pub fn advance_with_social(
        &mut self,
        contracts: &[Contract],
        events: &[AgreementEvent],
        encounters: &[game_social::Encounter],
        tick: u64,
    ) {
        #[cfg(feature = "profile")]
        let mut social_phase = game_diagnostics::span("opinions.social.affinity");
        if self.tracks_affinity_encounters() && !encounters.is_empty() {
            let unpressured: Vec<_> = encounters
                .par_iter()
                .flat_map_iter(|event| {
                    [(event.a, event.b, 1), (event.b, event.a, 1)]
                        .into_iter()
                        .filter(|row| !event.is_pressured(row.0))
                })
                .collect();
            let pressured: Vec<_> = encounters
                .par_iter()
                .flat_map_iter(|event| {
                    event.pressured_people().map(|person| {
                        (person, if person == event.a { event.b } else { event.a }, 1)
                    })
                })
                .collect();
            harm::affinity(self, &unpressured, harm::AffinityKind::Encounter);
            harm::affinity(self, &pressured, harm::AffinityKind::PressuredEncounter);
        }
        #[cfg(feature = "profile")]
        {
            drop(social_phase);
            social_phase = game_diagnostics::span("opinions.social.trust_prepare");
        }
        if (self.rules.encounter_gain == 0 || encounters.is_empty())
            && events
                .par_iter()
                .all(|event| !matches!(event.kind, EventKind::Fulfilled | EventKind::Defaulted))
        {
            self.events.clear();
            return;
        }
        let ordered = if contracts.par_windows(2).any(|p| p[0].id > p[1].id) {
            let mut rows: Vec<_> = contracts.par_iter().collect();
            rows.par_sort_unstable_by_key(|c| c.id);
            Some(rows)
        } else {
            None
        };
        let dense = if ordered.is_none() {
            contracts.first().and_then(|first| {
                contracts.last().and_then(|last| {
                    (last.id.checked_sub(first.id) == Some((contracts.len() - 1) as u64))
                        .then_some(first.id)
                })
            })
        } else {
            None
        };
        let find = |id: u64| -> &Contract {
            if let Some(rows) = &ordered {
                rows[rows
                    .binary_search_by_key(&id, |row| row.id)
                    .expect("validated agreement event")]
            } else {
                let index = dense
                    .and_then(|first| id.checked_sub(first))
                    .and_then(|n| usize::try_from(n).ok())
                    .filter(|&i| i < contracts.len())
                    .or_else(|| {
                        if dense.is_none() {
                            contracts.binary_search_by_key(&id, |c| c.id).ok()
                        } else {
                            None
                        }
                    })
                    .expect("validated agreement event");
                &contracts[index]
            }
        };
        // Index only configured personal interpretations; the usual authored
        // defaults need no lookup for every contract.
        let mut people: Vec<_> = if events.is_empty() {
            Vec::new()
        } else {
            self.sensitivities
                .par_iter()
                .filter(|p| {
                    p.trust_promise_fulfilled.is_some() || p.trust_promise_unfulfilled.is_some()
                })
                .collect()
        };
        if people.par_windows(2).any(|p| p[0].person > p[1].person) {
            people.par_sort_unstable_by_key(|p| p.person);
        }
        let mut cases: Vec<_> = if events.is_empty() {
            Vec::new()
        } else {
            self.sensitivities
                .par_iter()
                .flat_map_iter(|p| {
                    p.relationship_cases
                        .iter()
                        .filter(|case| {
                            case.promise_fulfilled_trust.is_some()
                                || case.promise_unfulfilled_trust.is_some()
                        })
                        .map(move |case| (p.person, case))
                })
                .collect()
        };
        if cases
            .par_windows(2)
            .any(|p| (p[0].0, p[0].1.target) > (p[1].0, p[1].1.target))
        {
            cases.par_sort_unstable_by_key(|row| (row.0, row.1.target));
        }
        let mut changes: Vec<_> = events
            .par_iter()
            .filter_map(|event| {
                if !matches!(event.kind, EventKind::Fulfilled | EventKind::Defaulted) {
                    return None;
                }
                let contract = find(event.contract);
                let pair = (contract.creditor, contract.debtor);
                let personal = people
                    .binary_search_by_key(&pair.0, |row| row.person)
                    .ok()
                    .map(|i| people[i]);
                let case = cases
                    .binary_search_by_key(&pair, |row| (row.0, row.1.target))
                    .ok()
                    .map(|i| cases[i].1);
                let delta = match event.kind {
                    EventKind::Fulfilled => case
                        .and_then(|c| c.promise_fulfilled_trust)
                        .or_else(|| personal.and_then(|p| p.trust_promise_fulfilled))
                        .map_or(i128::from(self.rules.fulfilled_gain), i128::from),
                    EventKind::Defaulted => case
                        .and_then(|c| c.promise_unfulfilled_trust)
                        .or_else(|| personal.and_then(|p| p.trust_promise_unfulfilled))
                        .map_or(-i128::from(self.rules.default_loss), i128::from),
                    _ => unreachable!(),
                };
                (delta != 0).then_some((pair, delta))
            })
            .collect();
        if self.rules.encounter_gain > 0 {
            let gain = i128::from(self.rules.encounter_gain);
            changes.par_extend(encounters.par_iter().flat_map_iter(|event| {
                [((event.a, event.b), gain), ((event.b, event.a), gain)]
                    .into_iter()
                    .filter(|(pair, _)| !event.is_pressured(pair.0))
            }));
        }
        if changes.is_empty() {
            self.events.clear();
            return;
        }
        if changes.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
            changes.par_sort_unstable_by_key(|change| change.0);
        }
        let updates: Vec<_> = if changes.par_windows(2).any(|pair| pair[0].0 == pair[1].0) {
            changes
                .par_chunk_by(|a, b| a.0 == b.0)
                .map(|group| {
                    (
                        group[0].0,
                        if group.len() >= 2048 {
                            group.par_iter().map(|row| row.1).sum::<i128>()
                        } else {
                            group.iter().map(|row| row.1).sum::<i128>()
                        },
                    )
                })
                .collect()
        } else {
            changes
        };
        #[cfg(feature = "profile")]
        {
            drop(social_phase);
            social_phase = game_diagnostics::span("opinions.social.trust_merge");
        }
        if self.records.is_empty() {
            let rules = &self.rules;
            self.events = updates
                .par_iter()
                .map(|&((observer, target), delta)| Event {
                    tick,
                    observer,
                    target,
                    before: rules.initial,
                    after: (i128::from(rules.initial) + delta)
                        .clamp(i128::from(rules.minimum), i128::from(rules.maximum))
                        as i32,
                })
                .collect();
            self.records = self
                .events
                .par_iter()
                .map(|event| Opinion {
                    observer: event.observer,
                    target: event.target,
                    trust: event.after,
                    fear: 0,
                    obligation: 0,
                    affinity: 0,
                })
                .collect();
            return;
        }
        let mut old = std::mem::take(&mut self.records);
        if old
            .par_windows(2)
            .any(|p| (p[0].observer, p[0].target) > (p[1].observer, p[1].target))
        {
            old.par_sort_unstable_by_key(|row| (row.observer, row.target));
        }
        (self.records, self.events) = social_merge::merge(old, &updates, &self.rules, tick);
        #[cfg(feature = "profile")]
        drop(social_phase);
    }
}

/// Clear transient receipts even when no agreement owner is enabled.
pub fn advance_optional(
    opinions: &mut Option<Opinions>,
    agreements: Option<&game_agreements::Agreements>,
    encounters: &[game_social::Encounter],
    tick: u64,
) {
    if let Some(opinions) = opinions {
        let (contracts, events) = agreements.map_or((&[][..], &[][..]), |owner| {
            (owner.contracts.as_slice(), owner.events.as_slice())
        });
        opinions.advance_with_social(contracts, events, encounters, tick);
        opinions.decay_fear();
    }
}

#[cfg(test)]
#[path = "../tests/support/owner.rs"]
mod tests;
