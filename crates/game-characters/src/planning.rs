//! Personal planning over owned memories and explicitly supplied opportunities.
//! Executable actions are opaque; no API command or world object enters here.
use crate::decisions::*;
use game_core::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
mod stocks;
pub use stocks::*;
mod storage;
use game_ecs::{Component, Store};
pub use storage::*;
#[cfg(test)]
mod storage_tests;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mind<A> {
    pub interests: BTreeMap<ChildhoodInterest, u64>,
    pub routes: BTreeMap<AreaId, BTreeMap<AreaId, SimDuration>>,
    pub last_failure: Option<(SimTime, InventoryId)>,
    pub child_signals: BTreeMap<PersonId, ChildSignal>,
    pub personality: Personality,
    pub dream: Aspiration,
    pub sites: BTreeMap<SiteId, SiteKnowledge>,
    pub relationships: BTreeMap<PersonId, Relationship>,
    pub values: BTreeMap<ConceptId, Attachment>,
    pub goal: Goal,
    pub plan: Option<Plan<A>>,
    pub explanation: Explanation,
    pub retry_after: SimTime,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Runtime {
    pub concepts: BTreeMap<ConceptId, Good>,
    pub work_concept: ConceptId,
    pub last_learning_event: u64,
    pub switching_margin: i64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum NeedRef {
    Hunger,
    Hydration,
    Recovery,
    Exposure,
    Company,
    Freedom,
}

pub(crate) fn resource_need_pressure(
    good: Good,
    hunger: u64,
    meal_relief: u64,
    hydration_urgency: i64,
) -> i64 {
    match good {
        Good::Food if hunger >= meal_relief => 10_000 + (hunger / 100).min(10_000) as i64,
        Good::Water if hydration_urgency > 0 => 10_000 + hydration_urgency,
        _ => 0,
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Capability {
    Skill(Skill),
    Interest(ChildhoodInterest),
    Livelihood,
    Movement,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CommitmentRef {
    Obligation(ObligationId),
    Project(ProjectId),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Subject {
    Inventory(InventoryId),
    Site(SiteId),
    Project(ProjectId),
    Resource(ResourceId),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Motive {
    Relieve {
        need: NeedRef,
        beneficiary: PersonId,
    },
    Develop {
        capability: Capability,
        beneficiary: PersonId,
    },
    Provide {
        requirement: game_logistics::Requirement,
        beneficiary: PersonId,
        need: Option<NeedRef>,
    },
    Fulfil {
        commitment: CommitmentRef,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum IntentionKey {
    Relief {
        need: NeedRef,
        beneficiary: PersonId,
    },
    Development {
        capability: Capability,
        beneficiary: PersonId,
        subject: Option<Subject>,
    },
    Provision {
        good: Good,
        beneficiary: PersonId,
        source: Option<InventoryId>,
    },
    Commitment(CommitmentRef),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Effort {
    Restoration,
    Light,
    Practice,
    Strenuous,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Uncertainty {
    Observed,
    Remembered,
    Conditional,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Risk {
    Safe,
    PhysicalDanger,
    UnconsentedTaking,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prospects {
    pub duration: SimDuration,
    pub travel: SimDuration,
    pub effort: Effort,
    pub uncertainty: Uncertainty,
    pub risk: Risk,
    pub source: Option<InventoryId>,
    pub site: Option<SiteId>,
    pub sponsor: Option<PersonId>,
}
impl Default for Prospects {
    fn default() -> Self {
        Self {
            duration: SimDuration(0),
            travel: SimDuration(0),
            effort: Effort::Light,
            uncertainty: Uncertainty::Observed,
            risk: Risk::Safe,
            source: None,
            site: None,
            sponsor: None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionShape {
    Eat(InventoryId),
    Move(AreaId),
    Work(SiteId),
    Care(PersonId),
    Comfort(PersonId),
    Rest,
    Stop,
    Resume,
    Other,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan<A> {
    pub action: A,
    pub key: IntentionKey,
    pub motives: Vec<Motive>,
    pub prospects: Prospects,
    pub shape: ActionShape,
    pub delivery: Option<game_logistics::Delivery>,
}
#[derive(Clone, Debug)]
pub struct Candidate<A> {
    pub action: A,
    pub key: IntentionKey,
    pub motives: Vec<Motive>,
    pub prospects: Prospects,
    pub shape: ActionShape,
    pub delivery: Option<game_logistics::Delivery>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activity {
    Idle,
    Rest,
    Working,
    Committed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Area(AreaId),
    Moving,
    Stopped,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    Autonomous,
    Manual,
    Delegated,
}
#[derive(Clone, Copy, Debug)]
pub struct NeedLimits {
    pub meal_relief: u64,
    pub hunger_harm: u64,
    pub meal_size: Quantity,
    pub eating_time: SimDuration,
    pub fatigue_limit: u64,
    pub max_health: u64,
    pub drink_quantity: Quantity,
}
#[derive(Clone, Debug)]
pub struct PersonalFacts {
    pub actor: PersonId,
    pub now: SimTime,
    pub vitals: Vitals,
    pub limits: NeedLimits,
    pub activity: Activity,
    pub place: Place,
    pub control: Control,
    pub in_dialogue: bool,
    pub independent: bool,
    pub child: bool,
    pub hunger_exempt: bool,
    pub work_units: u64,
    pub own_cargo: BTreeSet<InventoryId>,
    pub observed_contacts: BTreeSet<PersonId>,
    pub can_resume: bool,
    pub hydration_urgency: i64,
    pub exposure_urgency: i64,
    pub freedom_restricted: bool,
}
#[derive(Clone, Debug)]
pub struct Decision<A> {
    pub plan: Option<Plan<A>>,
    pub explanation: Explanation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CareDelivery {
    pub delivery: game_logistics::Delivery,
    pub location: Location,
    pub observed_at: SimTime,
}
fn care_urgency(signal: &ChildSignal, good: Good) -> i64 {
    match good {
        Good::Food => signal.urgency,
        Good::Water => signal.hydration_urgency,
        _ => 0,
    }
}
impl<A: Clone> Mind<A> {
    /// Motivation describes requirements only. Acquisition, carrying and travel
    /// are composed by logistics; absence supplies no new information.
    pub fn care_deliveries(&self, facts: &PersonalFacts) -> Vec<CareDelivery> {
        let mut goals = Vec::new();
        for (child, signal) in &self.child_signals {
            if *child == facts.actor
                || signal.observed_at > facts.now
                || self.care_motivation(*child) < 300
            {
                continue;
            }
            let fresh = signal.observed_at == facts.now && facts.observed_contacts.contains(child);
            for good in [Good::Food, Good::Water] {
                let continuing = self
                    .plan
                    .as_ref()
                    .filter(|plan| {
                        plan.key
                            == IntentionKey::Provision {
                                beneficiary: *child,
                                good,
                                source: None,
                            }
                    })
                    .and_then(|plan| plan.delivery.as_ref())
                    .filter(|delivery| delivery.remaining.0 > 0);
                if care_urgency(signal, good) == 0 || !fresh && continuing.is_none() {
                    continue;
                }
                let delivery = continuing.cloned().or_else(|| {
                    game_logistics::Delivery::new(
                        game_logistics::Requirement {
                            good,
                            quantity: if good == Good::Water {
                                facts.limits.drink_quantity
                            } else {
                                facts.limits.meal_size
                            },
                            source: None,
                        },
                        game_logistics::Destination::Contact(*child),
                    )
                    .ok()
                });
                if let Some(delivery) = delivery {
                    goals.push(CareDelivery {
                        delivery,
                        location: signal.location.clone(),
                        observed_at: signal.observed_at,
                    });
                }
            }
        }
        goals
    }
    /// Fresh local observation can remove the only applicable care signal.
    /// Missing information while away is retained by the observer, so this
    /// does not inspect another person's current body or require continued care.
    pub fn reconcile_care_observation(&mut self, actor: PersonId) {
        if let Some(plan) = &self.plan
            && let IntentionKey::Provision {
                beneficiary, good, ..
            } = plan.key
            && beneficiary != actor
            && plan.delivery.is_some()
            && self
                .child_signals
                .get(&beneficiary)
                .is_none_or(|signal| care_urgency(signal, good) == 0)
        {
            self.plan = None;
            self.goal = Goal::Rest;
            self.explanation = explanation(Goal::Rest, 0, 0, 0);
        }
    }
    pub fn accepts_risk(&self, risk: Risk) -> bool {
        self.personality.accepts_risk(risk)
    }
    pub fn record_care_receipt(
        &mut self,
        child: PersonId,
        good: Good,
        quantity: Quantity,
        sequence: u64,
    ) -> Result<bool, Error> {
        let Some(delivery) = self.plan.as_mut().and_then(|plan| plan.delivery.as_mut()) else {
            return Ok(false);
        };
        if delivery.destination != game_logistics::Destination::Contact(child)
            || delivery.requirement.good != good
        {
            return Ok(false);
        }
        delivery.delivered(&game_logistics::DeliveryReceipt {
            destination: delivery.destination,
            good,
            quantity,
            sequence,
        })
    }

    /// Providers need not assemble opportunities when this person cannot revise
    /// their action. Captivity still permits consideration of a physical escape.
    pub fn needs_candidates(&self, facts: &PersonalFacts) -> bool {
        let own_urgent = facts.vitals.hunger >= facts.limits.meal_relief;
        let critical_hydration = self.child_signals.iter().any(|(child, signal)| {
            signal.hydration_urgency >= 20_000
                && signal.observed_at == facts.now
                && facts.observed_contacts.contains(child)
        });
        let critical_child = critical_hydration
            || self
                .child_signals
                .values()
                .any(|signal| signal.urgency >= 20_000);
        if facts.in_dialogue
            && !own_urgent
            && facts.vitals.fatigue < facts.limits.fatigue_limit
            && facts.vitals.health >= facts.limits.max_health / 2
            && !critical_child
        {
            return false;
        }
        if facts.control != Control::Autonomous || !facts.vitals.alive || facts.vitals.health == 0 {
            return false;
        }
        if facts.freedom_restricted {
            return true;
        }
        if !facts.independent {
            return false;
        }
        if !matches!(facts.place, Place::Area(_)) {
            return matches!(facts.activity, Activity::Idle | Activity::Rest);
        }
        facts.activity != Activity::Committed
            && !(facts.activity == Activity::Working && !own_urgent && !critical_hydration)
    }

    pub fn decide(
        &self,
        facts: &PersonalFacts,
        stocks: &Stocks,
        candidates: &[Candidate<A>],
        work_concept: ConceptId,
        margin: i64,
    ) -> Decision<A> {
        let none = || Decision {
            plan: None,
            explanation: self.explanation.clone(),
        };
        let own_urgent = facts.vitals.hunger >= facts.limits.meal_relief;
        let critical_hydration = self.child_signals.iter().any(|(child, signal)| {
            signal.hydration_urgency >= 20_000
                && signal.observed_at == facts.now
                && facts.observed_contacts.contains(child)
        });
        if !self.needs_candidates(facts) {
            return none();
        }
        if let Some(candidate) = candidates
            .iter()
            .find(|candidate| candidate.motives.iter().any(|motive|matches!(motive,Motive::Relieve {need:NeedRef::Freedom,beneficiary} if *beneficiary==facts.actor)) && facts.freedom_restricted && self.accepts_risk(candidate.prospects.risk))
        {
            return Decision {
                plan: Some(Plan {
                    action: candidate.action.clone(),
                    key:candidate.key,motives:candidate.motives.clone(),prospects:candidate.prospects.clone(),
                    shape: candidate.shape,
                    delivery: candidate.delivery.clone(),
                }),
                explanation: explanation(Goal::Practice, 2_000, 0, 0),
            };
        }
        if !facts.independent {
            return none();
        }
        let on_road = !matches!(facts.place, Place::Area(_));
        let contacted = |child: &PersonId, signal: &ChildSignal| {
            *child != facts.actor
                && signal.observed_at == facts.now
                && facts.observed_contacts.contains(child)
        };
        let road_distress = self.child_signals.iter().any(|(child, signal)| {
            contacted(child, signal)
                && (signal.urgency > 0
                    || signal.hydration_urgency > 0
                    || signal.preference == Some(ChildhoodInterest::Company))
        });
        if on_road {
            if !matches!(facts.activity, Activity::Idle | Activity::Rest) {
                return none();
            }
        } else if facts.activity == Activity::Committed
            || facts.activity == Activity::Working && !own_urgent && !critical_hydration
        {
            return none();
        }
        if facts.place == Place::Moving {
            return candidates
                .iter()
                .find_map(|candidate| {
                    if candidate.shape != ActionShape::Stop {
                        return None;
                    }
                    self.score(
                        facts,
                        stocks,
                        candidate,
                        work_concept,
                        contacted,
                        road_distress,
                    )
                    .map(|explanation| Decision {
                        plan: Some(Plan {
                            action: candidate.action.clone(),
                            key: candidate.key,
                            motives: candidate.motives.clone(),
                            prospects: candidate.prospects.clone(),
                            shape: candidate.shape,
                            delivery: candidate.delivery.clone(),
                        }),
                        explanation,
                    })
                })
                .unwrap_or_else(none);
        }
        let mut scored = Vec::new();
        for candidate in candidates {
            let score = self.score(
                facts,
                stocks,
                candidate,
                work_concept,
                contacted,
                road_distress,
            );
            if let Some(score) = score {
                scored.push((candidate, score));
            }
        }
        let scores: Vec<_> = scored
            .iter()
            .map(|(candidate, score)| CandidateScore {
                total: score.total,
                tie: tie(candidate.shape, on_road),
                resting: candidate.shape == ActionShape::Rest,
            })
            .collect();
        let urgent = own_urgent
            || on_road
                && self.child_signals.iter().any(|(child, signal)| {
                    contacted(child, signal) && (signal.urgency > 0 || signal.hydration_urgency > 0)
                })
            || critical_hydration;
        let Some(index) = choose(
            &scores,
            ChoiceContext {
                working: !on_road && facts.activity == Activity::Working,
                resting: facts.activity == Activity::Rest,
                urgent,
                previous_score: self.explanation.total,
                switching_margin: margin,
            },
        ) else {
            return none();
        };
        let (candidate, score) = scored.swap_remove(index);
        Decision {
            plan: Some(Plan {
                action: candidate.action.clone(),
                key: candidate.key,
                motives: candidate.motives.clone(),
                prospects: candidate.prospects.clone(),
                shape: candidate.shape,
                delivery: candidate.delivery.clone(),
            }),
            explanation: score,
        }
    }
    fn score(
        &self,
        facts: &PersonalFacts,
        stocks: &Stocks,
        candidate: &Candidate<A>,
        work_concept: ConceptId,
        contacted: impl Fn(&PersonId, &ChildSignal) -> bool,
        road_distress: bool,
    ) -> Option<Explanation> {
        if !self.accepts_risk(candidate.prospects.risk) {
            return None;
        }
        let urgent = facts.vitals.hunger >= facts.limits.meal_relief;
        let road = !matches!(facts.place, Place::Area(_));
        if candidate.shape == ActionShape::Stop {
            return (facts.place == Place::Moving
                && self.child_signals.iter().any(|(child, signal)| {
                    contacted(child, signal)
                        && (signal.urgency > 0
                            || signal.hydration_urgency > 0
                            || signal.preference == Some(ChildhoodInterest::Company))
                        && self.care_motivation(*child) >= 300
                }))
            .then(|| explanation(Goal::Care, 3_000, 0, 0));
        }
        if candidate.shape == ActionShape::Resume {
            return (facts.place == Place::Stopped
                && facts.vitals.fatigue == 0
                && facts.vitals.health == facts.limits.max_health
                && !road_distress
                && facts.can_resume)
                .then(|| explanation(Goal::Survival, 0, 100, 0));
        }
        if let Some(site) = candidate.prospects.site {
            let site = self.sites.get(&site)?;
            let Place::Area(area) = facts.place else {
                return None;
            };
            if facts.child
                || site.area != area
                || !site.access
                || !stocks
                    .known
                    .get(&site.input)
                    .is_some_and(|stock| stock.accessible && stock.quantity >= site.input_quantity)
                || !stocks
                    .known
                    .get(&site.tool)
                    .is_some_and(|stock| stock.accessible && stock.quantity.0 >= 1_000)
            {
                return None;
            }
        }
        let mut scores = Vec::new();
        for motive in &candidate.motives {
            let score = match motive {
                Motive::Relieve { need, beneficiary } => {
                    let pressure =
                        self.need_pressure(*need, *beneficiary, facts, candidate.key, &contacted)?;
                    if *beneficiary == facts.actor {
                        if pressure == 0 && *need != NeedRef::Recovery {
                            continue;
                        }
                        if *need == NeedRef::Hunger && facts.hunger_exempt {
                            continue;
                        }
                        if let Some(source) = candidate.prospects.source {
                            let fact = stocks.known.get(&source)?;
                            let good = match need {
                                NeedRef::Hunger => Some(Good::Food),
                                NeedRef::Hydration => Some(Good::Water),
                                _ => None,
                            };
                            if good.is_some_and(|good| fact.good != good)
                                || !fact.accessible
                                || fact.quantity
                                    < if *need == NeedRef::Hydration {
                                        facts.limits.drink_quantity
                                    } else {
                                        facts.limits.meal_size
                                    }
                            {
                                continue;
                            }
                            if road
                                && (!facts.own_cargo.contains(&source)
                                    || fact.source != KnowledgeSource::Observed(facts.actor))
                            {
                                continue;
                            }
                        }
                        if *need == NeedRef::Hunger {
                            let travel = if let (Place::Area(area), Some(source)) =
                                (facts.place, candidate.prospects.source)
                            {
                                let fact = stocks.known.get(&source)?;
                                if fact.area != area {
                                    known_path(&self.routes, area, fact.area).ok()?.1.0
                                } else {
                                    0
                                }
                            } else {
                                candidate.prospects.travel.0
                            };
                            if travel.saturating_add(facts.limits.eating_time.0)
                                > facts
                                    .limits
                                    .hunger_harm
                                    .saturating_sub(facts.vitals.hunger)
                                    .saturating_add(facts.vitals.health)
                            {
                                continue;
                            }
                        }
                        let benefit = if candidate.prospects.effort == Effort::Restoration {
                            i64::from(self.personality.hedonism) / 3
                        } else {
                            0
                        };
                        Some(explanation(
                            if matches!(need, NeedRef::Recovery | NeedRef::Exposure) {
                                Goal::Recovery
                            } else {
                                Goal::Survival
                            },
                            pressure,
                            benefit,
                            0,
                        ))
                    } else {
                        let motive = self.care_motivation(*beneficiary);
                        (motive >= 300 && pressure > 0)
                            .then(|| explanation(Goal::Care, pressure, motive, 0))
                    }
                }
                Motive::Provide {
                    requirement,
                    beneficiary,
                    need,
                } => {
                    if requirement.quantity.0 == 0 {
                        continue;
                    }
                    if let Some(need) = need {
                        let pressure = self.need_pressure(
                            *need,
                            *beneficiary,
                            facts,
                            candidate.key,
                            &contacted,
                        )?;
                        let attachment = if *beneficiary == facts.actor {
                            0
                        } else {
                            self.care_motivation(*beneficiary)
                        };
                        if pressure == 0 || *beneficiary != facts.actor && attachment < 300 {
                            continue;
                        }
                        if candidate.shape == ActionShape::Care(*beneficiary)
                            && let Some(source) = candidate.prospects.source
                        {
                            let stock = stocks.known.get(&source)?;
                            if stock.good != requirement.good
                                || !stock.accessible
                                || stock.quantity < facts.limits.meal_size
                                || road
                                    && (!facts.own_cargo.contains(&source)
                                        || stock.source != KnowledgeSource::Observed(facts.actor))
                            {
                                continue;
                            }
                        }
                        Some(explanation(
                            if *beneficiary == facts.actor {
                                Goal::Survival
                            } else {
                                Goal::Care
                            },
                            pressure,
                            attachment,
                            0,
                        ))
                    } else {
                        let held = facts
                            .own_cargo
                            .iter()
                            .filter_map(|id| stocks.known.get(id))
                            .filter(|stock| {
                                stock.good == requirement.good
                                    && stock.source == KnowledgeSource::Observed(facts.actor)
                            })
                            .map(|stock| u128::from(stock.quantity.0))
                            .sum::<u128>();
                        if *beneficiary == facts.actor && held >= u128::from(requirement.quantity.0)
                        {
                            continue;
                        }
                        let interested = if *beneficiary == facts.actor {
                            self.dream == Aspiration::Independence
                                && self.personality.ambition >= 600
                        } else {
                            self.care_motivation(*beneficiary) >= 300
                        };
                        interested.then(|| {
                            explanation(
                                Goal::Practice,
                                0,
                                i64::from(self.personality.ambition) / 2,
                                0,
                            )
                        })
                    }
                }
                Motive::Develop {
                    capability,
                    beneficiary,
                } => {
                    if road
                        || urgent
                        || facts.now < self.retry_after
                        || facts.vitals.fatigue >= facts.limits.fatigue_limit
                    {
                        continue;
                    }
                    let other = *beneficiary != facts.actor;
                    let benefit = match capability {
                        Capability::Livelihood => {
                            let desired = if other {
                                self.dream == Aspiration::SupportOthers
                                    && self.personality.empathy >= 700
                            } else {
                                self.dream == Aspiration::Independence
                                    && self.personality.ambition >= 600
                                    || self.personality.discipline >= 700
                            };
                            if !desired {
                                continue;
                            }
                            i64::from(if other {
                                self.personality.empathy
                            } else {
                                self.personality.ambition
                            }) + 200
                        }
                        Capability::Interest(interest) => {
                            if !facts.child {
                                continue;
                            }
                            let inclination = match interest {
                                ChildhoodInterest::Company => self.personality.sociability,
                                ChildhoodInterest::Craft => self.personality.discipline,
                                ChildhoodInterest::Movement => self.personality.risk_tolerance,
                                ChildhoodInterest::Food => self.personality.hedonism,
                            };
                            i64::from(inclination) / 2
                                + self.interests.get(interest).copied().unwrap_or(0) as i64 / 4
                        }
                        Capability::Skill(skill) => {
                            let base =
                                i64::from(self.personality.discipline + self.personality.ambition)
                                    / 4;
                            if *skill == Skill::Craft {
                                base + if self.dream == Aspiration::Independence {
                                    100
                                } else {
                                    0
                                } + (facts.work_units / 10_000).min(100) as i64
                            } else {
                                base + 100
                            }
                        }
                        Capability::Movement => i64::from(self.personality.risk_tolerance) / 2,
                    };
                    let value = if *capability == Capability::Skill(Skill::Craft) {
                        self.values
                            .get(&work_concept)
                            .map_or(0, |value| i64::from(value.weight))
                            + self
                                .interests
                                .get(&ChildhoodInterest::Craft)
                                .copied()
                                .unwrap_or(0) as i64
                                / 4
                    } else {
                        0
                    };
                    Some(explanation(Goal::Practice, 0, benefit, value))
                }
                Motive::Fulfil { .. } => {
                    if urgent
                        || facts.vitals.fatigue >= facts.limits.fatigue_limit
                        || self.personality.discipline < 500 && self.personality.honesty < 500
                    {
                        continue;
                    }
                    Some(explanation(
                        Goal::Practice,
                        0,
                        i64::from(self.personality.discipline.max(self.personality.honesty)) + 200,
                        0,
                    ))
                }
            };
            if let Some(score) = score {
                scores.push(score);
            }
        }
        let mut result = scores.into_iter().max_by_key(|score| score.total)?;
        let time = candidate
            .prospects
            .travel
            .0
            .saturating_add(candidate.prospects.duration.0);
        let cost = (time / 1_000).min(1_000) as i64
            + match candidate.prospects.uncertainty {
                Uncertainty::Observed => 0,
                Uncertainty::Remembered => 25,
                Uncertainty::Conditional => 100,
            }
            + match candidate.prospects.effort {
                Effort::Restoration | Effort::Light => 0,
                Effort::Practice => 0,
                Effort::Strenuous => 50,
            };
        result.benefit -= cost;
        result.total = result.need + result.benefit + result.value;
        Some(result)
    }
    fn need_pressure(
        &self,
        need: NeedRef,
        beneficiary: PersonId,
        facts: &PersonalFacts,
        key: IntentionKey,
        contacted: &impl Fn(&PersonId, &ChildSignal) -> bool,
    ) -> Option<i64> {
        if beneficiary == facts.actor {
            return Some(match need {
                NeedRef::Hunger => resource_need_pressure(
                    Good::Food,
                    facts.vitals.hunger,
                    facts.limits.meal_relief,
                    facts.hydration_urgency,
                ),
                NeedRef::Hydration => resource_need_pressure(
                    Good::Water,
                    facts.vitals.hunger,
                    facts.limits.meal_relief,
                    facts.hydration_urgency,
                ),
                NeedRef::Exposure => facts.exposure_urgency,
                NeedRef::Recovery => (facts.vitals.fatigue / 500
                    + facts.limits.max_health.saturating_sub(facts.vitals.health) / 100)
                    .min(40_000) as i64,
                NeedRef::Freedom => {
                    if facts.freedom_restricted {
                        20_000
                    } else {
                        0
                    }
                }
                NeedRef::Company => i64::from(self.personality.sociability) / 2,
            });
        }
        let signal = self.child_signals.get(&beneficiary)?;
        let continuing = self.plan.as_ref().is_some_and(|plan| {
            plan.key == key
                && plan
                    .delivery
                    .as_ref()
                    .is_some_and(|delivery| delivery.remaining.0 > 0)
        });
        if !contacted(&beneficiary, signal) && !(continuing && signal.observed_at <= facts.now) {
            return None;
        }
        Some(match need {
            NeedRef::Hunger => signal.urgency,
            NeedRef::Hydration => signal.hydration_urgency,
            NeedRef::Company if signal.preference == Some(ChildhoodInterest::Company) => 500,
            _ => 0,
        })
    }
    pub fn record_attempt(
        &mut self,
        now: SimTime,
        plan: Plan<A>,
        mut explanation: Explanation,
        accepted: bool,
    ) -> Result<(), Error> {
        let retry = if accepted {
            self.retry_after
        } else {
            SimTime(now.0.checked_add(120_000).ok_or(Error::Overflow)?)
        };
        if !accepted {
            explanation.blocker = Some("physical attempt failed".into());
        }
        self.goal = explanation.goal;
        self.plan = Some(plan);
        self.explanation = explanation;
        self.retry_after = retry;
        Ok(())
    }
    fn care_motivation(&self, child: PersonId) -> i64 {
        i64::from(self.personality.empathy) / 2
            + i64::from(
                self.relationships
                    .get(&child)
                    .map_or(0, |relation| relation.affection.max(0)),
            )
    }
    pub fn observe_stock(
        &mut self,
        stocks: &mut Stocks,
        now: SimTime,
        here: AreaId,
        meal_size: Quantity,
        fact: StockKnowledge,
    ) {
        if self.goal == Goal::Survival
            && self
                .plan
                .as_ref()
                .is_some_and(|plan| plan.shape == ActionShape::Move(here))
            && stocks
                .known
                .get(&fact.inventory)
                .is_some_and(|old| old.good == Good::Food && old.quantity >= meal_size)
            && fact.quantity < meal_size
        {
            self.last_failure = Some((now, fact.inventory));
        }
        stocks.known.insert(fact.inventory, fact);
    }
    pub fn receive_report(
        &mut self,
        stocks: &mut Stocks,
        sender: PersonId,
        now: SimTime,
        mut fact: StockKnowledge,
    ) -> bool {
        fact.source = KnowledgeSource::Reported(sender);
        fact.learned_at = now;
        fact.accessible = stocks
            .known
            .get(&fact.inventory)
            .is_some_and(|known| known.accessible);
        let relationship = self.relationships.entry(sender).or_insert(Relationship {
            trust: 0,
            affection: 0,
        });
        if relationship.trust < -500 {
            return false;
        }
        relationship.trust = (relationship.trust + 10).min(1_000);
        if stocks
            .known
            .get(&fact.inventory)
            .is_none_or(|old| old.observed_at <= fact.observed_at)
        {
            stocks.known.insert(fact.inventory, fact);
        }
        true
    }
}
impl Runtime {
    pub fn learn_production<A: Component>(&self, store: &mut Store, actor: PersonId) {
        if let Some(mind) = mind_mut::<A>(store, actor) {
            mind.learn_production(self.work_concept);
        }
    }
}
impl<A> Mind<A> {
    pub fn learn_production(&mut self, work_concept: ConceptId) {
        let attachment = self.values.entry(work_concept).or_insert(Attachment {
            weight: 0,
            experiences: 0,
        });
        attachment.weight = (attachment.weight + 100).min(1_000);
        attachment.experiences = attachment.experiences.saturating_add(1);
    }
}

pub fn validate_runtime(runtime: &Runtime, next_event: u64) -> Result<(), Error> {
    if !runtime.concepts.contains_key(&runtime.work_concept)
        || runtime.last_learning_event >= next_event
        || runtime.switching_margin < 0
    {
        return Err(Error::InvalidSave(
            "inconsistent character decision metadata".into(),
        ));
    }
    Ok(())
}
fn explanation(goal: Goal, need: i64, benefit: i64, value: i64) -> Explanation {
    Explanation {
        goal,
        need,
        benefit,
        value,
        total: need + benefit + value,
        blocker: None,
    }
}
fn tie(shape: ActionShape, road: bool) -> (u8, u64) {
    if road {
        match shape {
            ActionShape::Care(child) | ActionShape::Comfort(child) => (0, child.0),
            ActionShape::Eat(source) => (1, source.0),
            ActionShape::Rest => (2, 0),
            _ => (3, 0),
        }
    } else {
        match shape {
            ActionShape::Eat(source) => (0, source.0),
            ActionShape::Move(destination) => (1, destination.0),
            ActionShape::Work(site) => (2, site.0),
            _ => (3, 0),
        }
    }
}
pub fn known_path(
    graph: &BTreeMap<AreaId, BTreeMap<AreaId, SimDuration>>,
    origin: AreaId,
    destination: AreaId,
) -> Result<(Vec<AreaId>, SimDuration), Error> {
    game_spatial::shortest_path(graph, origin, destination)
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn mind() -> Mind<u8> {
        Mind {
            interests: BTreeMap::new(),
            routes: BTreeMap::new(),
            last_failure: None,
            child_signals: BTreeMap::new(),
            personality: Personality::industrious(),
            dream: Aspiration::Independence,
            sites: BTreeMap::new(),
            relationships: BTreeMap::new(),
            values: BTreeMap::new(),
            goal: Goal::Rest,
            plan: None,
            explanation: explanation(Goal::Rest, 0, 0, 0),
            retry_after: SimTime(0),
        }
    }

    #[test]
    fn candidate_admission_skips_inactive_people_but_preserves_a_captives_escape() {
        let mind = mind();
        let mut facts = facts();
        let escape = Candidate {
            action: 9,
            key: IntentionKey::Relief {
                need: NeedRef::Freedom,
                beneficiary: facts.actor,
            },
            motives: vec![Motive::Relieve {
                need: NeedRef::Freedom,
                beneficiary: facts.actor,
            }],
            prospects: Prospects::default(),
            shape: ActionShape::Other,
            delivery: None,
        };
        for control in [Control::Manual, Control::Delegated] {
            facts.control = control;
            assert!(!mind.needs_candidates(&facts));
            assert!(
                mind.decide(
                    &facts,
                    &Stocks::default(),
                    std::slice::from_ref(&escape),
                    ConceptId(1),
                    0
                )
                .plan
                .is_none()
            );
        }
        facts.control = Control::Autonomous;
        facts.activity = Activity::Committed;
        assert!(!mind.needs_candidates(&facts));
        facts.freedom_restricted = true;
        assert!(mind.needs_candidates(&facts));
        assert_eq!(
            mind.decide(&facts, &Stocks::default(), &[escape], ConceptId(1), 0)
                .plan
                .unwrap()
                .action,
            9
        );
        facts.vitals.alive = false;
        assert!(!mind.needs_candidates(&facts));
    }

    #[test]
    fn working_caregivers_request_opportunities_for_fresh_critical_distress() {
        let mut mind = mind();
        let mut facts = facts();
        facts.activity = Activity::Working;
        assert!(!mind.needs_candidates(&facts));
        let child = PersonId(9);
        mind.child_signals.insert(
            child,
            ChildSignal {
                location: Location::AtArea(AreaId(1)),
                urgency: 0,
                hydration_urgency: 20_000,
                preference: None,
                observed_at: facts.now,
            },
        );
        assert!(!mind.needs_candidates(&facts));
        facts.observed_contacts.insert(child);
        assert!(mind.needs_candidates(&facts));
        facts.now.0 += 1;
        assert!(!mind.needs_candidates(&facts));
        facts.vitals.hunger = facts.limits.meal_relief;
        assert!(mind.needs_candidates(&facts));
    }

    #[test]
    fn chosen_care_declares_resource_requirements_without_acquire_or_return_workflows() {
        for (good, food, water) in [(Good::Food, 3_000, 0), (Good::Water, 0, 3_000)] {
            let child = PersonId(9);
            let mut m = mind();
            let mut f = facts();
            f.limits.drink_quantity = Quantity(250);
            f.observed_contacts.insert(child);
            m.relationships.insert(
                child,
                Relationship {
                    affection: 700,
                    trust: 700,
                },
            );
            m.child_signals.insert(
                child,
                ChildSignal {
                    location: Location::AtArea(AreaId(1)),
                    observed_at: f.now,
                    urgency: food,
                    hydration_urgency: water,
                    preference: None,
                },
            );
            let goal = m.care_deliveries(&f).pop().unwrap();
            assert_eq!(goal.delivery.requirement.good, good);
            let dose = if good == Good::Water {
                f.limits.drink_quantity
            } else {
                f.limits.meal_size
            };
            assert_eq!(goal.delivery.requirement.quantity, dose);
            let supply = Candidate {
                action: 1,
                key: IntentionKey::Provision {
                    good,
                    beneficiary: child,
                    source: None,
                },
                motives: vec![Motive::Provide {
                    requirement: goal.delivery.requirement.clone(),
                    beneficiary: child,
                    need: Some(if good == Good::Food {
                        NeedRef::Hunger
                    } else {
                        NeedRef::Hydration
                    }),
                }],
                prospects: Prospects::default(),
                shape: ActionShape::Move(AreaId(2)),
                delivery: Some(goal.delivery),
            };
            let d = m.decide(
                &f,
                &Stocks::default(),
                std::slice::from_ref(&supply),
                ConceptId(1),
                0,
            );
            m.record_attempt(f.now, d.plan.unwrap(), d.explanation, true)
                .unwrap();
            f.now = SimTime(200);
            f.place = Place::Area(AreaId(2));
            f.observed_contacts.clear();
            let remembered = m.care_deliveries(&f).pop().unwrap();
            assert_eq!(remembered.location, Location::AtArea(AreaId(1)));
            assert_eq!(remembered.observed_at, SimTime(100));
            assert!(
                m.decide(
                    &f,
                    &Stocks::default(),
                    std::slice::from_ref(&supply),
                    ConceptId(1),
                    0
                )
                .plan
                .is_some()
            );
            assert!(m.record_care_receipt(child, good, dose, 1).unwrap());
            assert!(m.care_deliveries(&f).is_empty());
        }
    }
    #[test]
    fn fresh_observation_removing_an_applicable_care_signal_abandons_only_that_goal() {
        let child = PersonId(9);
        let mut m = mind();
        let mut f = facts();
        f.observed_contacts.insert(child);
        m.relationships.insert(
            child,
            Relationship {
                affection: 700,
                trust: 700,
            },
        );
        m.child_signals.insert(
            child,
            ChildSignal {
                location: Location::AtArea(AreaId(1)),
                observed_at: f.now,
                urgency: 3_000,
                hydration_urgency: 0,
                preference: None,
            },
        );
        let goal = m.care_deliveries(&f).pop().unwrap();
        let supply = Candidate {
            action: 1,
            key: IntentionKey::Provision {
                good: Good::Food,
                beneficiary: child,
                source: None,
            },
            motives: vec![Motive::Provide {
                requirement: goal.delivery.requirement.clone(),
                beneficiary: child,
                need: Some(NeedRef::Hunger),
            }],
            prospects: Prospects::default(),
            shape: ActionShape::Move(AreaId(2)),
            delivery: Some(goal.delivery),
        };
        let decision = m.decide(&f, &Stocks::default(), &[supply], ConceptId(1), 0);
        m.record_attempt(f.now, decision.plan.unwrap(), decision.explanation, true)
            .unwrap();
        // While absent, the last contact remains a dated reason to continue.
        m.reconcile_care_observation(f.actor);
        assert!(m.plan.is_some());
        // The observer later removes the signal after local inapplicability.
        m.child_signals.remove(&child);
        m.reconcile_care_observation(f.actor);
        assert!(m.plan.is_none());
        assert_eq!(m.goal, Goal::Rest);
    }
    pub(super) fn facts() -> PersonalFacts {
        PersonalFacts {
            actor: PersonId(1),
            now: SimTime(100),
            vitals: Vitals {
                alive: true,
                health: 10_000,
                hunger: 0,
                fatigue: 0,
            },
            limits: NeedLimits {
                meal_relief: 10_000,
                hunger_harm: 20_000,
                meal_size: Quantity(1_000),
                eating_time: SimDuration(1_000),
                fatigue_limit: 50_000,
                max_health: 10_000,
                drink_quantity: Quantity(1_000),
            },
            activity: Activity::Idle,
            place: Place::Area(AreaId(1)),
            control: Control::Autonomous,
            in_dialogue: false,
            independent: true,
            child: false,
            hunger_exempt: false,
            work_units: 0,
            own_cargo: BTreeSet::new(),
            observed_contacts: BTreeSet::new(),
            can_resume: false,
            hydration_urgency: 0,
            exposure_urgency: 0,
            freedom_restricted: false,
        }
    }
    fn stock(id: u64, good: Good, area: u64, quantity: u64) -> StockKnowledge {
        StockKnowledge {
            inventory: InventoryId(id),
            good,
            area: AreaId(area),
            quantity: Quantity(quantity),
            accessible: true,
            observed_at: SimTime(1),
            learned_at: SimTime(1),
            source: KnowledgeSource::Observed(PersonId(1)),
        }
    }
    #[derive(Clone, Copy)]
    enum Example {
        Food(InventoryId),
        Care {
            child: PersonId,
            source: InventoryId,
        },
        Rest,
        Work(SiteId),
        Stop,
        Resume,
    }
    fn candidate(action: u8, example: Example, shape: ActionShape) -> Candidate<u8> {
        let actor = PersonId(1);
        let mut prospects = Prospects::default();
        let (key, motive) = match example {
            Example::Food(source) => {
                prospects.source = Some(source);
                (
                    IntentionKey::Relief {
                        need: NeedRef::Hunger,
                        beneficiary: actor,
                    },
                    Motive::Relieve {
                        need: NeedRef::Hunger,
                        beneficiary: actor,
                    },
                )
            }
            Example::Care { child, source } => {
                prospects.source = Some(source);
                (
                    IntentionKey::Provision {
                        good: Good::Water,
                        beneficiary: child,
                        source: None,
                    },
                    Motive::Provide {
                        requirement: game_logistics::Requirement {
                            good: Good::Water,
                            quantity: Quantity(1_000),
                            source: None,
                        },
                        beneficiary: child,
                        need: Some(NeedRef::Hydration),
                    },
                )
            }
            Example::Rest => {
                prospects.effort = Effort::Restoration;
                (
                    IntentionKey::Relief {
                        need: NeedRef::Recovery,
                        beneficiary: actor,
                    },
                    Motive::Relieve {
                        need: NeedRef::Recovery,
                        beneficiary: actor,
                    },
                )
            }
            Example::Work(site) => {
                prospects.site = Some(site);
                prospects.effort = Effort::Practice;
                (
                    IntentionKey::Development {
                        capability: Capability::Skill(Skill::Craft),
                        beneficiary: actor,
                        subject: Some(Subject::Site(site)),
                    },
                    Motive::Develop {
                        capability: Capability::Skill(Skill::Craft),
                        beneficiary: actor,
                    },
                )
            }
            Example::Stop | Example::Resume => (
                IntentionKey::Development {
                    capability: Capability::Movement,
                    beneficiary: actor,
                    subject: None,
                },
                Motive::Develop {
                    capability: Capability::Movement,
                    beneficiary: actor,
                },
            ),
        };
        let shape = match example {
            Example::Stop => ActionShape::Stop,
            Example::Resume => ActionShape::Resume,
            _ => shape,
        };
        Candidate {
            action,
            key,
            motives: vec![motive],
            prospects,
            shape,
            delivery: None,
        }
    }
    fn work(m: &mut Mind<u8>, stocks: &mut Stocks) -> Candidate<u8> {
        stocks
            .known
            .insert(InventoryId(3), stock(3, Good::Timber, 1, 1_000));
        stocks
            .known
            .insert(InventoryId(4), stock(4, Good::Tool, 1, 1_000));
        m.sites.insert(
            SiteId(2),
            SiteKnowledge {
                site: SiteId(2),
                area: AreaId(1),
                input: InventoryId(3),
                tool: InventoryId(4),
                output: InventoryId(5),
                input_quantity: Quantity(1_000),
                access: true,
            },
        );
        candidate(2, Example::Work(SiteId(2)), ActionShape::Work(SiteId(2)))
    }
    fn choose(
        m: &Mind<u8>,
        stocks: &Stocks,
        f: &PersonalFacts,
        c: &[Candidate<u8>],
    ) -> Decision<u8> {
        m.decide(f, stocks, c, ConceptId(6), 100)
    }
    #[test]
    fn complete_selection_uses_personality_experience_and_actual_known_opportunities() {
        let mut m = mind();
        let mut stocks = Stocks::default();
        let f = facts();
        let work = work(&mut m, &mut stocks);
        let rest = candidate(1, Example::Rest, ActionShape::Rest);
        assert_eq!(
            choose(&m, &stocks, &f, &[rest.clone(), work.clone()])
                .plan
                .unwrap()
                .action,
            2
        );
        m.personality = Personality::restful();
        m.dream = Aspiration::Comfort;
        assert_eq!(
            choose(&m, &stocks, &f, &[rest.clone(), work.clone()])
                .plan
                .unwrap()
                .action,
            1
        );
        let mut schema = game_ecs::Schema::new();
        register_minds::<u8>(&mut schema).unwrap();
        let mut store = Store::with_schema(schema);
        insert_mind(&mut store, f.actor, m).unwrap();
        insert_stocks(&mut store, f.actor, stocks).unwrap();
        let world = Runtime {
            concepts: BTreeMap::from([(ConceptId(6), Good::Timber)]),
            work_concept: ConceptId(6),
            last_learning_event: 0,
            switching_margin: 100,
        };
        for _ in 0..3 {
            world.learn_production::<u8>(&mut store, f.actor);
        }
        assert_eq!(
            choose(
                super::mind::<u8>(&store, f.actor).unwrap(),
                super::stocks(&store, f.actor).unwrap(),
                &f,
                &[rest.clone(), work.clone()]
            )
            .plan
            .unwrap()
            .action,
            2
        );
        assert_eq!(
            super::mind::<u8>(&store, f.actor).unwrap().values[&ConceptId(6)].experiences,
            3
        );
        stocks_mut(&mut store, f.actor)
            .unwrap()
            .known
            .get_mut(&InventoryId(4))
            .unwrap()
            .quantity = Quantity(0);
        assert_eq!(
            choose(
                super::mind::<u8>(&store, f.actor).unwrap(),
                super::stocks(&store, f.actor).unwrap(),
                &f,
                &[rest, work]
            )
            .plan
            .unwrap()
            .action,
            1
        );
    }
    #[test]
    fn planning_does_not_grant_remote_facts_or_routes_and_reports_do_not_grant_access() {
        let mut m = mind();
        let mut stocks = Stocks::default();
        let mut f = facts();
        f.vitals.hunger = 10_000;
        let food = candidate(
            9,
            Example::Food(InventoryId(9)),
            ActionShape::Move(AreaId(2)),
        );
        assert!(
            choose(&m, &stocks, &f, std::slice::from_ref(&food))
                .plan
                .is_none()
        );
        let snapshot = stock(9, Good::Food, 2, 1_000);
        assert!(m.receive_report(&mut stocks, PersonId(2), f.now, snapshot.clone()));
        assert!(!stocks.known[&InventoryId(9)].accessible);
        assert_eq!(stocks.known[&InventoryId(9)].observed_at, SimTime(1));
        stocks.known.get_mut(&InventoryId(9)).unwrap().accessible = true;
        assert!(
            choose(&m, &stocks, &f, std::slice::from_ref(&food))
                .plan
                .is_none()
        );
        m.routes.insert(
            AreaId(1),
            BTreeMap::from([(AreaId(2), SimDuration(25_000))]),
        );
        assert!(
            choose(&m, &stocks, &f, std::slice::from_ref(&food))
                .plan
                .is_none()
        );
        m.routes
            .get_mut(&AreaId(1))
            .unwrap()
            .insert(AreaId(2), SimDuration(1_000));
        let decision = choose(&m, &stocks, &f, &[food]);
        m.record_attempt(f.now, decision.plan.unwrap(), decision.explanation, true)
            .unwrap();
        let mut empty = snapshot;
        empty.quantity = Quantity(0);
        m.observe_stock(&mut stocks, f.now, AreaId(2), f.limits.meal_size, empty);
        assert_eq!(m.last_failure, Some((f.now, InventoryId(9))));
        m.relationships.get_mut(&PersonId(2)).unwrap().trust = -501;
        let before = m.clone();
        assert!(!m.receive_report(
            &mut stocks,
            PersonId(2),
            f.now,
            stock(9, Good::Food, 2, 1_000)
        ));
        assert_eq!(m, before);
    }
    #[test]
    fn failed_plan_has_cooldown_and_overflow_is_inert() {
        let mut m = mind();
        let mut stocks = Stocks::default();
        let mut f = facts();
        let work = work(&mut m, &mut stocks);
        let d = choose(&m, &stocks, &f, std::slice::from_ref(&work));
        m.record_attempt(f.now, d.plan.unwrap(), d.explanation, false)
            .unwrap();
        assert_eq!(m.retry_after, SimTime(120_100));
        assert_eq!(
            m.explanation.blocker.as_deref(),
            Some("physical attempt failed")
        );
        assert!(
            choose(&m, &stocks, &f, std::slice::from_ref(&work))
                .plan
                .is_none()
        );
        f.now = m.retry_after;
        let d = choose(&m, &stocks, &f, &[work]);
        assert!(d.plan.is_some());
        let before = m.clone();
        assert_eq!(
            m.record_attempt(SimTime(u64::MAX), d.plan.unwrap(), d.explanation, false),
            Err(Error::Overflow)
        );
        assert_eq!(m, before);
    }
    #[test]
    fn active_commitments_and_hysteresis_preserve_the_existing_plan() {
        let mut m = mind();
        let mut stocks = Stocks::default();
        let mut f = facts();
        let work = work(&mut m, &mut stocks);
        f.activity = Activity::Committed;
        assert!(
            choose(&m, &stocks, &f, std::slice::from_ref(&work))
                .plan
                .is_none()
        );
        f.activity = Activity::Working;
        m.explanation.total = 10_000;
        assert!(
            choose(&m, &stocks, &f, std::slice::from_ref(&work))
                .plan
                .is_none()
        );
        stocks
            .known
            .insert(InventoryId(9), stock(9, Good::Food, 1, 1_000));
        f.vitals.hunger = 10_000;
        assert_eq!(
            choose(
                &m,
                &stocks,
                &f,
                &[
                    candidate(
                        9,
                        Example::Food(InventoryId(9)),
                        ActionShape::Eat(InventoryId(9))
                    ),
                    work
                ]
            )
            .plan
            .unwrap()
            .action,
            9
        );
        f.control = Control::Manual;
        assert!(
            choose(
                &m,
                &stocks,
                &f,
                &[candidate(1, Example::Rest, ActionShape::Rest)]
            )
            .plan
            .is_none()
        );
    }
    #[test]
    fn hydration_requires_observed_contact_and_personal_care_motivation() {
        let mut m = mind();
        let mut stocks = Stocks::default();
        let mut f = facts();
        let child = PersonId(2);
        m.child_signals.insert(
            child,
            ChildSignal {
                preference: None,
                location: Location::AtArea(AreaId(1)),
                urgency: 0,
                hydration_urgency: 20_000,
                observed_at: f.now,
            },
        );
        stocks
            .known
            .insert(InventoryId(9), stock(9, Good::Water, 1, 1_000));
        let care = candidate(
            9,
            Example::Care {
                child,
                source: InventoryId(9),
            },
            ActionShape::Care(child),
        );
        assert!(
            choose(&m, &stocks, &f, std::slice::from_ref(&care))
                .plan
                .is_none()
        );
        f.observed_contacts.insert(child);
        assert!(
            choose(&m, &stocks, &f, std::slice::from_ref(&care))
                .plan
                .is_none()
        );
        m.relationships.insert(
            child,
            Relationship {
                trust: 0,
                affection: 100,
            },
        );
        f.activity = Activity::Working;
        assert_eq!(
            choose(&m, &stocks, &f, std::slice::from_ref(&care))
                .plan
                .unwrap()
                .action,
            9
        );
        m.child_signals.get_mut(&child).unwrap().observed_at = SimTime(0);
        assert!(choose(&m, &stocks, &f, &[care]).plan.is_none());
    }
    #[test]
    fn road_choices_stop_before_care_and_do_not_use_origin_warehouses() {
        let mut m = mind();
        let mut stocks = Stocks::default();
        let mut f = facts();
        let child = PersonId(2);
        f.observed_contacts.insert(child);
        m.personality.empathy = 800;
        m.child_signals.insert(
            child,
            ChildSignal {
                preference: None,
                location: Location::AtArea(AreaId(1)),
                urgency: 0,
                hydration_urgency: 3_000,
                observed_at: f.now,
            },
        );
        stocks
            .known
            .insert(InventoryId(9), stock(9, Good::Water, 1, 1_000));
        let care = candidate(
            9,
            Example::Care {
                child,
                source: InventoryId(9),
            },
            ActionShape::Care(child),
        );
        let stop = candidate(1, Example::Stop, ActionShape::Other);
        f.place = Place::Moving;
        assert!(
            choose(&m, &stocks, &f, std::slice::from_ref(&care))
                .plan
                .is_none()
        );
        assert_eq!(
            choose(&m, &stocks, &f, &[care.clone(), stop])
                .plan
                .unwrap()
                .action,
            1
        );
        f.place = Place::Stopped;
        assert!(
            choose(&m, &stocks, &f, std::slice::from_ref(&care))
                .plan
                .is_none()
        );
        f.own_cargo.insert(InventoryId(9));
        assert_eq!(choose(&m, &stocks, &f, &[care]).plan.unwrap().action, 9);
        f.can_resume = true;
        let resume = candidate(3, Example::Resume, ActionShape::Other);
        assert!(
            choose(&m, &stocks, &f, std::slice::from_ref(&resume))
                .plan
                .is_none()
        );
        m.child_signals.clear();
        assert_eq!(choose(&m, &stocks, &f, &[resume]).plan.unwrap().action, 3);
    }
    #[test]
    fn complete_personal_planning_state_roundtrips_without_world_or_api_types() {
        let mut m = mind();
        let mut stocks = Stocks::default();
        let f = facts();
        let work = work(&mut m, &mut stocks);
        let d = choose(&m, &stocks, &f, &[work]);
        m.record_attempt(f.now, d.plan.unwrap(), d.explanation, true)
            .unwrap();
        let restored: (Mind<u8>, Stocks) =
            serde_json::from_slice(&serde_json::to_vec(&(&m, &stocks)).unwrap()).unwrap();
        assert_eq!((m.clone(), stocks), restored);
        let aspiration: crate::social::Aspiration = m.dream;
        assert_eq!(aspiration, Aspiration::Independence);
    }
}

impl<A> Mind<A> {
    pub fn offers_teaching(&self) -> bool {
        self.personality.empathy + self.personality.patience >= 800
    }
    pub fn develop_aspiration(
        &mut self,
        learned: &crate::social::Development,
        control: Control,
        independent: bool,
        vitals: &Vitals,
        margin: u32,
    ) {
        if control == Control::Autonomous
            && independent
            && vitals.alive
            && vitals.health > 0
            && let Some(aspiration) = learned.preferred_aspiration(self.dream, margin)
        {
            self.dream = aspiration;
        }
    }
}

#[cfg(test)]
mod social_choice_tests {
    use super::tests::{facts, mind};
    use super::*;
    use crate::social::{
        ChoiceFacts, Development, InstructionTopic, Offer, SocialAction, SocialTopic, Temperament,
    };
    fn social(mind: &Mind<u8>) -> crate::social::World {
        crate::social::World {
            development: BTreeMap::from([(
                PersonId(1),
                Development::baseline(
                    Temperament {
                        discipline: 900,
                        ambition: 900,
                        sociability: 500,
                        hedonism: 100,
                        empathy: 500,
                    },
                    mind.dream,
                )
                .unwrap(),
            )]),
            ..Default::default()
        }
    }
    fn contacts() -> ChoiceFacts {
        ChoiceFacts {
            actor: PersonId(1),
            now: SimTime(0),
            autonomous: true,
            can_speak: true,
            adult: true,
            family_peers: vec![PersonId(3), PersonId(2)],
            available_teachers: BTreeSet::new(),
            own_levels: BTreeMap::new(),
            can_practice_craft: false,
            companionship_partner: Some(PersonId(2)),
        }
    }
    #[test]
    fn social_planning_owns_family_approaches_without_assuming_recipient_consent() {
        let mut mind = mind();
        mind.dream = Aspiration::Parenthood;
        let mut world = social(&mind);
        let mut contact = contacts();
        assert_eq!(
            world.choose_personal(&mind, &contact),
            Some(SocialAction::Talk {
                partner: PersonId(2),
                topic: SocialTopic::Companionship
            })
        );
        world
            .companionship
            .insert(crate::social::pair(PersonId(1), PersonId(2)));
        mind.relationships.insert(
            PersonId(2),
            Relationship {
                trust: 100,
                affection: 100,
            },
        );
        assert_eq!(
            world.choose_personal(&mind, &contact),
            Some(SocialAction::Talk {
                partner: PersonId(2),
                topic: SocialTopic::FamilyPlans
            })
        );
        assert!(world.family_intent.is_empty());
        world
            .approaches
            .insert(PersonId(1), BTreeMap::from([(PersonId(2), SimTime(1))]));
        assert_eq!(
            world.choose_personal(&mind, &contact),
            Some(SocialAction::Talk {
                partner: PersonId(3),
                topic: SocialTopic::Companionship
            })
        );
        contact.autonomous = false;
        assert!(world.choose_personal(&mind, &contact).is_none());
    }
    #[test]
    fn teaching_requires_received_offer_and_current_opportunity_and_choice_waits() {
        let mind = mind();
        let mut world = social(&mind);
        let mut contact = contacts();
        contact.companionship_partner = None;
        contact.available_teachers.insert(PersonId(2));
        assert!(world.choose_personal(&mind, &contact).is_none());
        world.offers.insert(
            PersonId(1),
            BTreeMap::from([(
                PersonId(2),
                Offer {
                    teacher: PersonId(2),
                    topics: BTreeSet::from([InstructionTopic::Routes]),
                    area: AreaId(1),
                    heard_at: SimTime(0),
                },
            )]),
        );
        assert_eq!(
            world.choose_personal(&mind, &contact),
            Some(SocialAction::Learn {
                teacher: PersonId(2),
                topic: InstructionTopic::Routes
            })
        );
        contact.available_teachers.clear();
        assert!(world.choose_personal(&mind, &contact).is_none());
        contact.can_practice_craft = true;
        assert_eq!(
            world.choose_personal(&mind, &contact),
            Some(SocialAction::Practice {
                skill: Skill::Craft
            })
        );
        world
            .finish_personal_choice(contact.actor, contact.now)
            .unwrap();
        assert!(world.choose_personal(&mind, &contact).is_none());
        let before = world.clone();
        assert_eq!(
            world.finish_personal_choice(contact.actor, SimTime(u64::MAX)),
            Err(Error::Overflow)
        );
        assert_eq!(world, before);
    }
    #[test]
    fn aspiration_updates_from_learned_evidence_only_when_person_has_physical_agency() {
        let mut mind = mind();
        mind.dream = Aspiration::Comfort;
        let world = social(&mind);
        let mut learned = world.development[&PersonId(1)].clone();
        learned.observe(Aspiration::SupportOthers, 10_000, "chosen caregiving");
        let f = facts();
        mind.develop_aspiration(&learned, Control::Manual, true, &f.vitals, 50);
        assert_eq!(mind.dream, Aspiration::Comfort);
        mind.develop_aspiration(&learned, Control::Autonomous, false, &f.vitals, 50);
        assert_eq!(mind.dream, Aspiration::Comfort);
        mind.develop_aspiration(&learned, Control::Autonomous, true, &f.vitals, 50);
        assert_eq!(mind.dream, Aspiration::SupportOthers);
    }
}
