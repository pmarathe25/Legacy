//! Local hearings and per-person institution discovery from explicit facts.
use crate::institutions::*;
use crate::storage::*;
use crate::voting::VoterFacts;
use game_core::*;
use game_ecs::Store;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Copy)]
pub struct HearingActor {
    pub actor: PersonId,
    pub area: Option<AreaId>,
    pub autonomous: bool,
    pub empathy: u16,
}
#[derive(Clone, Copy)]
pub struct LivelihoodSite {
    pub site: SiteId,
    pub area: AreaId,
    pub accessible: bool,
}
#[derive(Clone, Copy)]
pub struct Recruit {
    pub actor: PersonId,
    pub area: Option<AreaId>,
    pub autonomous: bool,
    pub citizen: bool,
}
pub struct DiscoveryFacts<'a> {
    pub voter: &'a VoterFacts<'a>,
    pub now: SimTime,
    pub authority: &'a AuthorityFacts,
    pub references: &'a ReferenceFacts,
    pub sites: &'a [LivelihoodSite],
    /// Actual population order, preserving the original first-six recruitment.
    pub recruits: &'a [Recruit],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureHandling {
    Ignore,
    Propagate,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveryDecision {
    pub action: InstitutionAction,
    pub search_after: SimTime,
    pub failure: FailureHandling,
}
impl Runtime {
    pub fn hearing_intentions(
        &self,
        store: &Store,
        authority: &AuthorityFacts,
        actors: &[HearingActor],
    ) -> Result<Vec<(PersonId, InstitutionAction)>, Error> {
        let mut by_actor = BTreeMap::new();
        for actor in actors {
            if by_actor.insert(actor.actor, actor).is_some() {
                return Err(Error::Unavailable);
            }
        }
        let mut choices = Vec::new();
        for institution in crate::storage::institutions(store)
            .map(|(_, row)| row)
            .filter(|row| row.status == InstitutionStatus::Active)
        {
            for office in &institution.charter.offices {
                let Some(actor) = by_actor.get(&office.holder) else {
                    continue;
                };
                if !actor.autonomous
                    || !office.powers.contains(&OfficePower::HearAccessCases)
                    || !institution.office_effective(office, authority)
                {
                    continue;
                }
                let Some(memory) = crate::storage::knowledge(store, office.holder) else {
                    continue;
                };
                for id in memory.cases.iter().rev().take(64) {
                    let case = crate::storage::case(store, id).ok_or(Error::Unavailable)?;
                    if case.kind != SocialCaseKind::Exclusion
                        || case.outcome != SocialOutcome::Unresolved
                        || case.area != institution.charter.jurisdiction
                        || !institution.members.contains(&case.subject)
                        || crate::storage::hearing(store, id).is_some()
                        || case
                            .inventory
                            .is_none_or(|inventory| !office.inventories.contains(&inventory))
                        || actor.area != Some(case.area)
                    {
                        continue;
                    }
                    let score = i64::from(case.need_band) * 100 + i64::from(actor.empathy) - 800
                        + self.precedent_score(store, office.holder, case);
                    choices.push((
                        office.holder,
                        InstitutionAction::HearCase {
                            institution: institution.id,
                            office: office.slot,
                            case: *id,
                            share: score >= 0,
                        },
                    ));
                }
            }
        }
        Ok(choices)
    }
    /// Request actual references mechanically; selection and validation remain here.
    pub fn discovery_references(
        &self,
        store: &Store,
        actor: PersonId,
        area: AreaId,
        sites: &[LivelihoodSite],
        recruits: &[Recruit],
    ) -> ReferenceRequest {
        let mut request = ReferenceRequest::default();
        request.people.insert(actor);
        for institution in crate::storage::institutions(store)
            .map(|(_, row)| row)
            .filter(|row| {
                row.charter.jurisdiction == area && row.status == InstitutionStatus::Active
            })
        {
            request.charter(&institution.charter);
        }
        if let Some(memory) = crate::storage::knowledge(store, actor) {
            for id in &memory.cases {
                if let Some(case) = crate::storage::case(store, id) {
                    request.people.extend([case.subject, case.counterpart]);
                    request.stocks.extend(case.inventory);
                }
            }
        }
        request.sites.extend(sites.iter().map(|site| site.site));
        request.people.extend(recruits.iter().map(|row| row.actor));
        request
    }
    /// Call again after the previous person's action commits; this does not freeze a population-wide batch.
    pub fn discovery_decision(
        &self,
        store: &Store,
        facts: DiscoveryFacts<'_>,
    ) -> Option<DiscoveryDecision> {
        let voter = facts.voter;
        let actor = voter.actor;
        let area = voter.local_area?;
        if !voter.autonomous
            || crate::storage::knowledge(store, actor).is_none()
            || crate::storage::requests(store, actor).map_or(SimTime(0), |row| row.search_after)
                > facts.now
        {
            return None;
        }
        let search_after = SimTime(facts.now.0.saturating_add(300_000));
        for institution in crate::storage::institutions(store)
            .map(|(_, row)| row)
            .filter(|row| {
                row.charter.jurisdiction == area && row.status == InstitutionStatus::Active
            })
        {
            if !institution.members.contains(&actor)
                && institution.charter.open_membership
                && self.charter_vote_score(store, voter, &institution.charter) >= 0
            {
                return Some(DiscoveryDecision {
                    action: InstitutionAction::Join {
                        institution: institution.id,
                    },
                    search_after,
                    failure: FailureHandling::Ignore,
                });
            }
            if institution.members.contains(&actor)
                && institution
                    .subjects(&institution.charter.procedure.proposers, facts.authority)
                    .contains(&actor)
                && self.known_case_count(store, actor, actor, SocialCaseKind::Default) >= 2
                && let Some((index, old)) =
                    institution
                        .charter
                        .rules
                        .iter()
                        .enumerate()
                        .find(|(_, rule)| {
                            matches!(rule.effect, SocialEffect::Access { allowed: true, .. })
                                && !expression_has_defaults(&rule.when)
                        })
            {
                let mut rule = old.clone();
                rule.when = RuleExpression::All(vec![
                    old.when.clone(),
                    RuleExpression::Not(Box::new(RuleExpression::Fact(
                        SocialCondition::KnownDefaultsAtLeast(1),
                    ))),
                ]);
                if facts
                    .references
                    .validate_rule(&institution.charter, &rule)
                    .is_ok()
                {
                    return Some(DiscoveryDecision {
                        action: InstitutionAction::Propose {
                            institution: institution.id,
                            change: InstitutionChange::ReplaceRule {
                                index: index as u16,
                                rule,
                            },
                        },
                        search_after,
                        failure: FailureHandling::Ignore,
                    });
                }
            }
        }
        let case = crate::storage::knowledge(store, actor)?
            .cases
            .iter()
            .rev()
            .filter_map(|id| crate::storage::case(store, id))
            .find(|case| {
                case.kind == SocialCaseKind::Exclusion
                    && case.subject == actor
                    && case.outcome == SocialOutcome::Unresolved
                    && case.area == area
            })?;
        let inventory = case.inventory?;
        if crate::storage::institutions(store).map(|(_, row)| row).any(|institution|institution.charter.jurisdiction==area
            && matches!(institution.status,InstitutionStatus::Active|InstitutionStatus::Proposed)
            && institution.charter.rules.iter().any(|rule|matches!(rule.effect,SocialEffect::Access{inventory:target,..} if target==inventory))) {return None;}
        let owner = case.counterpart;
        if !facts
            .recruits
            .iter()
            .any(|row| row.actor == owner && row.autonomous && row.area == Some(area))
        {
            return None;
        }
        let site = facts
            .sites
            .iter()
            .filter(|row| row.area == area && row.accessible)
            .min_by_key(|row| row.site)?
            .site;
        let mut founders = BTreeSet::from([actor, owner]);
        founders.extend(
            facts
                .recruits
                .iter()
                .filter(|row| {
                    row.citizen
                        && row.autonomous
                        && row.area == Some(area)
                        && row.actor != actor
                        && row.actor != owner
                })
                .take(6)
                .map(|row| row.actor),
        );
        let hunger = RuleExpression::Fact(SocialCondition::HungerAtLeast(120_000));
        let stock = RuleExpression::Fact(SocialCondition::KnownStockAtLeast {
            inventory,
            quantity: Quantity(2_000),
        });
        let cooperation = RuleExpression::Fact(SocialCondition::KnownCooperationAtLeast(1));
        let eligible = if voter.empathy >= 800 {
            RuleExpression::Any(vec![hunger.clone(), cooperation])
        } else {
            cooperation
        };
        let mut share_conditions = vec![
            RuleExpression::Fact(SocialCondition::Member),
            stock.clone(),
            eligible,
        ];
        if self.known_case_count(store, actor, actor, SocialCaseKind::Default) > 0 {
            share_conditions.push(RuleExpression::Not(Box::new(RuleExpression::Fact(
                SocialCondition::KnownDefaultsAtLeast(1),
            ))));
        }
        let charter = InstitutionCharter {
            purpose: format!("Access and contribution at store {}", inventory.0),
            jurisdiction: area,
            founders: founders.clone(),
            open_membership: true,
            procedure: DecisionProcedure {
                proposers: Subjects::members(),
                voters: Subjects::members(),
                minimum_support: (founders.len() / 2 + 1) as u16,
            },
            offices: vec![OfficeMandate {
                slot: 0,
                holder: owner,
                inventories: BTreeSet::from([inventory]),
                sites: BTreeSet::from([site]),
                powers: BTreeSet::from([OfficePower::Access, OfficePower::Labor]),
                minimum_support: 2,
            }],
            rules: vec![
                InstitutionalRule {
                    subjects: Subjects::members(),
                    when: RuleExpression::All(vec![
                        RuleExpression::Fact(SocialCondition::Member),
                        hunger,
                        stock,
                        RuleExpression::Fact(SocialCondition::Excluded { inventory }),
                    ]),
                    effect: SocialEffect::Labor {
                        site,
                        millis: 60_000,
                        beneficiary: Beneficiary::Person(owner),
                        due_after: SimDuration(180_000),
                    },
                    authority: 0,
                },
                InstitutionalRule {
                    subjects: Subjects::members(),
                    when: RuleExpression::All(share_conditions),
                    effect: SocialEffect::Access {
                        inventory,
                        allowed: true,
                    },
                    authority: 0,
                },
            ],
            contributions: Vec::new(),
            evidence: BTreeSet::from([case.id]),
        };
        (self.charter_vote_score(store, voter, &charter) >= 0).then_some(DiscoveryDecision {
            action: InstitutionAction::Found {
                charter,
                origin: None,
            },
            search_after,
            failure: FailureHandling::Propagate,
        })
    }
    pub fn record_discovery_attempt(
        &self,
        store: &mut Store,
        actor: PersonId,
        decision: &DiscoveryDecision,
        accepted: bool,
    ) -> Result<(), Error> {
        if !accepted && decision.failure == FailureHandling::Propagate {
            return Err(Error::Unavailable);
        }
        if crate::storage::knowledge(store, actor).is_none() {
            return Err(Error::Unavailable);
        }
        insert_requests(
            store,
            actor,
            Requests {
                search_after: decision.search_after,
            },
        )?;
        Ok(())
    }
}
pub fn expression_has_defaults(expression: &RuleExpression) -> bool {
    match expression {
        RuleExpression::Fact(SocialCondition::KnownDefaultsAtLeast(_)) => true,
        RuleExpression::All(items) | RuleExpression::Any(items) => {
            items.iter().any(expression_has_defaults)
        }
        RuleExpression::Not(item) => expression_has_defaults(item),
        _ => false,
    }
}
#[cfg(test)]
mod tests;

pub struct RuleSpeaker<'a> {
    pub actor: PersonId,
    pub area: Option<AreaId>,
    pub hunger: u64,
    pub known_stocks: &'a BTreeMap<InventoryId, StockKnowledge>,
}
pub struct RuleApplications {
    pub institution: InstitutionId,
    pub intents: Vec<RuleIntent>,
}
impl Runtime {
    /// Rule publication does not alter hearing, physical liveness or personal stock observations.
    pub fn rule_applications(
        &self,
        store: &Store,
        authority: &AuthorityFacts,
        speakers: &[RuleSpeaker<'_>],
        locations: &BTreeMap<PersonId, AreaId>,
        at: SimTime,
    ) -> Result<Vec<RuleApplications>, Error> {
        let mut by_actor = BTreeMap::new();
        for speaker in speakers {
            if by_actor.insert(speaker.actor, speaker).is_some() {
                return Err(Error::Unavailable);
            }
        }
        let mut batches = Vec::new();
        for institution in crate::storage::institutions(store)
            .map(|(_, row)| row)
            .filter(|row| row.status == InstitutionStatus::Active)
        {
            let mut intents = Vec::new();
            for (index, rule) in institution.charter.rules.iter().enumerate() {
                let office = institution
                    .charter
                    .offices
                    .iter()
                    .find(|office| office.slot == rule.authority)
                    .ok_or(Error::Unavailable)?;
                let Some(speaker) = by_actor.get(&office.holder) else {
                    continue;
                };
                if !institution.office_effective(office, authority)
                    || speaker.area != Some(institution.charter.jurisdiction)
                {
                    continue;
                }
                for actor in institution.subjects(&rule.subjects, authority) {
                    let input = ConditionInput {
                        observer: office.holder,
                        subject: actor,
                        subject_local: locations.get(&actor)
                            == Some(&institution.charter.jurisdiction),
                        own_hunger: speaker.hunger,
                        known_stocks: speaker.known_stocks,
                    };
                    if let Some(intent) =
                        self.rule_intent(store, institution, index, &input, authority, true, at)
                    {
                        intents.push(intent);
                    }
                }
            }
            batches.push(RuleApplications {
                institution: institution.id,
                intents,
            });
        }
        Ok(batches)
    }
}

pub struct DutyWorker<'a> {
    pub actor: PersonId,
    pub autonomous: bool,
    pub area: Option<AreaId>,
    pub idle: bool,
    pub work_budget: SimDuration,
    pub discipline: u16,
    pub empathy: u16,
    pub sites: &'a [LivelihoodSite],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DutyWork {
    pub actor: PersonId,
    pub site: SiteId,
}
impl Runtime {
    pub fn duty_workers(
        &self,
        store: &Store,
        ledger: &game_agreements::State,
    ) -> Result<BTreeSet<PersonId>, Error> {
        let agreements = crate::storage::institutions(store)
            .map(|(_, row)| row)
            .filter(|institution| institution.status == InstitutionStatus::Active)
            .flat_map(|institution| {
                institution
                    .applied
                    .values()
                    .flat_map(|people| people.values().copied())
            })
            .collect::<BTreeSet<_>>();
        let mut people = BTreeSet::new();
        for id in agreements {
            let agreement = ledger.agreements.get(&id).ok_or(Error::Unavailable)?;
            for id in &agreement.obligations {
                if let game_agreements::AgreementTerm::Labor { debtor, .. } =
                    ledger.obligations.get(id).ok_or(Error::Unavailable)?.term
                {
                    people.insert(debtor);
                }
            }
        }
        Ok(people)
    }
    pub fn duty_work_intentions(
        &self,
        store: &Store,
        ledger: &game_agreements::State,
        workers: &[DutyWorker<'_>],
    ) -> Result<Vec<DutyWork>, Error> {
        let mut by_actor = BTreeMap::new();
        for worker in workers {
            if by_actor.insert(worker.actor, worker).is_some() {
                return Err(Error::Unavailable);
            }
        }
        let agreements = crate::storage::institutions(store)
            .map(|(_, row)| row)
            .filter(|institution| institution.status == InstitutionStatus::Active)
            .flat_map(|institution| {
                institution
                    .applied
                    .values()
                    .flat_map(|people| people.values().copied())
            })
            .collect::<BTreeSet<_>>();
        let mut intentions = BTreeMap::new();
        for agreement in agreements {
            let agreement = ledger
                .agreements
                .get(&agreement)
                .ok_or(Error::Unavailable)?;
            for id in &agreement.obligations {
                let obligation = ledger.obligations.get(id).ok_or(Error::Unavailable)?;
                let game_agreements::AgreementTerm::Labor {
                    debtor,
                    site,
                    millis,
                    ..
                } = obligation.term
                else {
                    continue;
                };
                if obligation.performed >= millis || intentions.contains_key(&debtor) {
                    continue;
                }
                let Some(worker) = by_actor.get(&debtor) else {
                    continue;
                };
                let Some(area) = worker.area else {
                    continue;
                };
                if !worker.autonomous
                    || !worker.idle
                    || worker.work_budget.0 < millis - obligation.performed
                    || (worker.discipline < 500 && worker.empathy < 500)
                    || !worker
                        .sites
                        .iter()
                        .any(|known| known.site == site && known.area == area && known.accessible)
                {
                    continue;
                }
                intentions.insert(debtor, site);
            }
        }
        Ok(intentions
            .into_iter()
            .map(|(actor, site)| DutyWork { actor, site })
            .collect())
    }
}
