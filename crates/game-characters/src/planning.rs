//! Personal planning over owned memories and explicitly supplied opportunities.
//! Executable actions are opaque; no API command or world object enters here.
use crate::decisions::*;
use game_core::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mind<A> {
    pub interests: BTreeMap<ChildhoodInterest, u64>,
    pub routes: BTreeMap<AreaId, BTreeMap<AreaId, SimDuration>>,
    pub last_failure: Option<(SimTime, InventoryId)>,
    pub child_signals: BTreeMap<PersonId, ChildSignal>,
    pub personality: Personality,
    pub dream: Aspiration,
    pub knowledge: BTreeMap<InventoryId, StockKnowledge>,
    pub sites: BTreeMap<SiteId, SiteKnowledge>,
    pub relationships: BTreeMap<PersonId, Relationship>,
    pub values: BTreeMap<ConceptId, Attachment>,
    pub goal: Goal,
    pub plan: Option<Plan<A>>,
    pub explanation: Explanation,
    pub retry_after: SimTime,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct World<A> {
    pub minds: BTreeMap<PersonId, Mind<A>>,
    pub concepts: BTreeMap<ConceptId, Good>,
    pub work_concept: ConceptId,
    pub last_learning_event: u64,
    pub switching_margin: i64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Purpose {
    Force,
    Health,
    Forage,
    Project,
    Property,
    Food(InventoryId),
    Care {
        child: PersonId,
        source: InventoryId,
    },
    Comfort(PersonId),
    Rest,
    Cultivate,
    Play(ChildhoodInterest),
    Work(SiteId),
    Stop,
    Resume,
    CareSupply(PersonId),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionShape {
    Eat(InventoryId),
    Move(AreaId),
    Work(SiteId),
    Care(PersonId),
    Comfort(PersonId),
    Rest,
    Other,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan<A> {
    pub action: A,
    pub purpose: Purpose,
    pub shape: ActionShape,
}
#[derive(Clone, Debug)]
pub struct Candidate<A> {
    pub action: A,
    pub purpose: Purpose,
    pub shape: ActionShape,
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
}
#[derive(Clone, Debug)]
pub struct Decision<A> {
    pub plan: Option<Plan<A>>,
    pub explanation: Explanation,
}
impl<A: Clone> Mind<A> {
    pub fn decide(
        &self,
        facts: &PersonalFacts,
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
            return none();
        }
        if facts.control != Control::Autonomous || !facts.vitals.alive || facts.vitals.health == 0 {
            return none();
        }
        if let Some(candidate) = candidates
            .iter()
            .find(|candidate| candidate.purpose == Purpose::Force)
        {
            return Decision {
                plan: Some(Plan {
                    action: candidate.action.clone(),
                    purpose: candidate.purpose,
                    shape: candidate.shape,
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
                    if candidate.purpose != Purpose::Stop {
                        return None;
                    }
                    self.score(facts, candidate, work_concept, contacted, road_distress)
                        .map(|explanation| Decision {
                            plan: Some(Plan {
                                action: candidate.action.clone(),
                                purpose: candidate.purpose,
                                shape: candidate.shape,
                            }),
                            explanation,
                        })
                })
                .unwrap_or_else(none);
        }
        let mut scored = Vec::new();
        for candidate in candidates {
            let score = self.score(facts, candidate, work_concept, contacted, road_distress);
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
                purpose: candidate.purpose,
                shape: candidate.shape,
            }),
            explanation: score,
        }
    }
    fn score(
        &self,
        facts: &PersonalFacts,
        candidate: &Candidate<A>,
        work_concept: ConceptId,
        contacted: impl Fn(&PersonId, &ChildSignal) -> bool,
        road_distress: bool,
    ) -> Option<Explanation> {
        let urgent = facts.vitals.hunger >= facts.limits.meal_relief;
        let road = !matches!(facts.place, Place::Area(_));
        let practice = facts.now >= self.retry_after
            && !urgent
            && facts.vitals.fatigue < facts.limits.fatigue_limit;
        let recovery = if road {
            (facts.vitals.fatigue / 500
                + facts.limits.max_health.saturating_sub(facts.vitals.health) / 100)
                .min(i64::MAX as u64) as i64
        } else {
            facts.vitals.fatigue as i64 / 500
                + (facts.limits.max_health - facts.vitals.health) as i64 / 100
        };
        match candidate.purpose {
            Purpose::Force => None,
            Purpose::Health if !road => Some(explanation(Goal::Recovery, 2_000, 200, 0)),
            Purpose::Forage if !road => Some(explanation(
                Goal::Survival,
                if urgent { 2_000 } else { 0 },
                200,
                0,
            )),
            Purpose::Project if !road => Some(explanation(
                Goal::Practice,
                0,
                i64::from(self.personality.ambition) + 200,
                0,
            )),
            Purpose::Property if !road => Some(explanation(Goal::Survival, 9_000, 0, 0)),
            Purpose::Food(inventory) if urgent && (!facts.hunger_exempt || road) => {
                let fact = self.knowledge.get(&inventory)?;
                if fact.good != Good::Food
                    || !fact.accessible
                    || fact.quantity < facts.limits.meal_size
                {
                    return None;
                }
                if road {
                    if fact.source != KnowledgeSource::Observed(facts.actor)
                        || !facts.own_cargo.contains(&inventory)
                    {
                        return None;
                    }
                    Some(explanation(
                        Goal::Survival,
                        10_000 + facts.vitals.hunger as i64 / 100,
                        0,
                        0,
                    ))
                } else {
                    let Place::Area(area) = facts.place else {
                        unreachable!()
                    };
                    let travel = if fact.area == area {
                        0
                    } else {
                        known_path(&self.routes, area, fact.area).ok()?.1.0
                    };
                    let survival = facts
                        .limits
                        .hunger_harm
                        .saturating_sub(facts.vitals.hunger)
                        .saturating_add(facts.vitals.health);
                    if travel.saturating_add(facts.limits.eating_time.0) > survival {
                        return None;
                    }
                    Some(explanation(
                        Goal::Survival,
                        facts.vitals.hunger as i64 / 100 + 10_000,
                        -((travel / 1_000).min(i64::MAX as u64) as i64),
                        0,
                    ))
                }
            }
            Purpose::Care { child, source } => {
                let signal = self.child_signals.get(&child)?;
                if !contacted(&child, signal) {
                    return None;
                }
                let motive = self.care_motivation(child);
                if motive < 300 {
                    return None;
                }
                let fact = self.knowledge.get(&source)?;
                if !fact.accessible {
                    return None;
                }
                let need = match fact.good {
                    Good::Food if fact.quantity >= facts.limits.meal_size => signal.urgency,
                    Good::Water if fact.quantity >= facts.limits.drink_quantity => {
                        signal.hydration_urgency
                    }
                    _ => return None,
                };
                if need == 0 {
                    return None;
                }
                if road
                    && (fact.source != KnowledgeSource::Observed(facts.actor)
                        || !facts.own_cargo.contains(&source))
                {
                    return None;
                }
                if let Place::Area(area) = facts.place
                    && fact.area != area
                {
                    return None;
                }
                Some(explanation(Goal::Care, need, motive, 0))
            }
            Purpose::Comfort(child) => {
                let signal = self.child_signals.get(&child)?;
                if !contacted(&child, signal)
                    || signal.preference != Some(ChildhoodInterest::Company)
                {
                    return None;
                }
                let motive = self.care_motivation(child);
                (motive >= 300).then(|| explanation(Goal::Care, 500, motive, 0))
            }
            Purpose::CareSupply(child) => {
                let signal = self.child_signals.get(&child)?;
                let motive = self.care_motivation(child);
                (contacted(&child, signal) && signal.hydration_urgency > 0 && motive >= 300)
                    .then(|| explanation(Goal::Care, signal.hydration_urgency, motive, 0))
            }
            Purpose::Rest if facts.place != Place::Moving => Some(explanation(
                Goal::Recovery,
                recovery,
                i64::from(self.personality.hedonism) / 3,
                0,
            )),
            Purpose::Cultivate if !road && practice => Some(explanation(
                Goal::Practice,
                0,
                i64::from(self.personality.discipline + self.personality.ambition) / 4,
                100,
            )),
            Purpose::Play(interest) if !road && practice && facts.child => {
                let inclination = match interest {
                    ChildhoodInterest::Company => self.personality.sociability,
                    ChildhoodInterest::Craft => self.personality.discipline,
                    ChildhoodInterest::Movement => self.personality.risk_tolerance,
                    ChildhoodInterest::Food => self.personality.hedonism,
                };
                Some(explanation(
                    Goal::Practice,
                    0,
                    i64::from(inclination) / 2,
                    self.interests.get(&interest).copied().unwrap_or(0) as i64 / 4,
                ))
            }
            Purpose::Work(site) if !road && practice && !facts.child => {
                let site = self.sites.get(&site)?;
                let Place::Area(area) = facts.place else {
                    unreachable!()
                };
                if site.area != area
                    || !site.access
                    || !self
                        .knowledge
                        .get(&site.input)
                        .is_some_and(|fact| fact.accessible && fact.quantity >= site.input_quantity)
                    || !self
                        .knowledge
                        .get(&site.tool)
                        .is_some_and(|fact| fact.accessible && fact.quantity.0 >= 1_000)
                {
                    return None;
                }
                let dream = if self.dream == Aspiration::Independence {
                    100
                } else {
                    0
                };
                Some(explanation(
                    Goal::Practice,
                    0,
                    i64::from(self.personality.discipline + self.personality.ambition) / 4
                        + dream
                        + (facts.work_units / 10_000).min(100) as i64,
                    self.values
                        .get(&work_concept)
                        .map_or(0, |value| i64::from(value.weight))
                        + self
                            .interests
                            .get(&ChildhoodInterest::Craft)
                            .copied()
                            .unwrap_or(0) as i64
                            / 4,
                ))
            }
            Purpose::Stop
                if facts.place == Place::Moving
                    && self.child_signals.iter().any(|(child, signal)| {
                        contacted(child, signal)
                            && (signal.urgency > 0
                                || signal.hydration_urgency > 0
                                || signal.preference == Some(ChildhoodInterest::Company))
                            && self.care_motivation(*child) >= 300
                    }) =>
            {
                Some(explanation(Goal::Care, 3_000, 0, 0))
            }
            Purpose::Resume
                if facts.place == Place::Stopped
                    && recovery == 0
                    && !road_distress
                    && facts.can_resume =>
            {
                Some(explanation(Goal::Survival, 0, 100, 0))
            }
            _ => None,
        }
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
            && self
                .knowledge
                .get(&fact.inventory)
                .is_some_and(|old| old.good == Good::Food && old.quantity >= meal_size)
            && fact.quantity < meal_size
        {
            self.last_failure = Some((now, fact.inventory));
        }
        self.knowledge.insert(fact.inventory, fact);
    }
    pub fn receive_report(
        &mut self,
        sender: PersonId,
        now: SimTime,
        mut fact: StockKnowledge,
    ) -> bool {
        fact.source = KnowledgeSource::Reported(sender);
        fact.learned_at = now;
        fact.accessible = self
            .knowledge
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
        if self
            .knowledge
            .get(&fact.inventory)
            .is_none_or(|old| old.observed_at <= fact.observed_at)
        {
            self.knowledge.insert(fact.inventory, fact);
        }
        true
    }
}
impl<A> World<A> {
    pub fn learn_production(&mut self, actor: PersonId) {
        if let Some(mind) = self.minds.get_mut(&actor) {
            let attachment = mind.values.entry(self.work_concept).or_insert(Attachment {
                weight: 0,
                experiences: 0,
            });
            attachment.weight = (attachment.weight + 100).min(1_000);
            attachment.experiences = attachment.experiences.saturating_add(1);
        }
    }
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
    let mut frontier = BTreeSet::from([(0_u64, Vec::<AreaId>::new(), origin)]);
    let mut visited = BTreeSet::new();
    while let Some((cost, path, area)) = frontier.pop_first() {
        if !visited.insert(area) {
            continue;
        }
        if area == destination {
            return Ok((path, SimDuration(cost)));
        }
        for (to, duration) in graph.get(&area).into_iter().flatten() {
            let mut next = path.clone();
            next.push(*to);
            frontier.insert((
                cost.checked_add(duration.0).ok_or(Error::Overflow)?,
                next,
                *to,
            ));
        }
    }
    Err(Error::NoRoute)
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
            knowledge: BTreeMap::new(),
            sites: BTreeMap::new(),
            relationships: BTreeMap::new(),
            values: BTreeMap::new(),
            goal: Goal::Rest,
            plan: None,
            explanation: explanation(Goal::Rest, 0, 0, 0),
            retry_after: SimTime(0),
        }
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
    fn candidate(action: u8, purpose: Purpose, shape: ActionShape) -> Candidate<u8> {
        Candidate {
            action,
            purpose,
            shape,
        }
    }
    fn work(m: &mut Mind<u8>) -> Candidate<u8> {
        m.knowledge
            .insert(InventoryId(3), stock(3, Good::Timber, 1, 1_000));
        m.knowledge
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
        candidate(2, Purpose::Work(SiteId(2)), ActionShape::Work(SiteId(2)))
    }
    fn choose(m: &Mind<u8>, f: &PersonalFacts, c: &[Candidate<u8>]) -> Decision<u8> {
        m.decide(f, c, ConceptId(6), 100)
    }
    #[test]
    fn complete_selection_uses_personality_experience_and_actual_known_opportunities() {
        let mut m = mind();
        let f = facts();
        let work = work(&mut m);
        let rest = candidate(1, Purpose::Rest, ActionShape::Rest);
        assert_eq!(
            choose(&m, &f, &[rest.clone(), work.clone()])
                .plan
                .unwrap()
                .action,
            2
        );
        m.personality = Personality::restful();
        m.dream = Aspiration::Comfort;
        assert_eq!(
            choose(&m, &f, &[rest.clone(), work.clone()])
                .plan
                .unwrap()
                .action,
            1
        );
        let mut world = World {
            minds: BTreeMap::from([(f.actor, m)]),
            concepts: BTreeMap::from([(ConceptId(6), Good::Timber)]),
            work_concept: ConceptId(6),
            last_learning_event: 0,
            switching_margin: 100,
        };
        for _ in 0..3 {
            world.learn_production(f.actor);
        }
        assert_eq!(
            choose(&world.minds[&f.actor], &f, &[rest.clone(), work.clone()])
                .plan
                .unwrap()
                .action,
            2
        );
        assert_eq!(world.minds[&f.actor].values[&ConceptId(6)].experiences, 3);
        world
            .minds
            .get_mut(&f.actor)
            .unwrap()
            .knowledge
            .get_mut(&InventoryId(4))
            .unwrap()
            .quantity = Quantity(0);
        assert_eq!(
            choose(&world.minds[&f.actor], &f, &[rest, work])
                .plan
                .unwrap()
                .action,
            1
        );
    }
    #[test]
    fn planning_does_not_grant_remote_facts_or_routes_and_reports_do_not_grant_access() {
        let mut m = mind();
        let mut f = facts();
        f.vitals.hunger = 10_000;
        let food = candidate(
            9,
            Purpose::Food(InventoryId(9)),
            ActionShape::Move(AreaId(2)),
        );
        assert!(choose(&m, &f, std::slice::from_ref(&food)).plan.is_none());
        let snapshot = stock(9, Good::Food, 2, 1_000);
        assert!(m.receive_report(PersonId(2), f.now, snapshot.clone()));
        assert!(!m.knowledge[&InventoryId(9)].accessible);
        assert_eq!(m.knowledge[&InventoryId(9)].observed_at, SimTime(1));
        m.knowledge.get_mut(&InventoryId(9)).unwrap().accessible = true;
        assert!(choose(&m, &f, std::slice::from_ref(&food)).plan.is_none());
        m.routes.insert(
            AreaId(1),
            BTreeMap::from([(AreaId(2), SimDuration(25_000))]),
        );
        assert!(choose(&m, &f, std::slice::from_ref(&food)).plan.is_none());
        m.routes
            .get_mut(&AreaId(1))
            .unwrap()
            .insert(AreaId(2), SimDuration(1_000));
        let decision = choose(&m, &f, &[food]);
        m.record_attempt(f.now, decision.plan.unwrap(), decision.explanation, true)
            .unwrap();
        let mut empty = snapshot;
        empty.quantity = Quantity(0);
        m.observe_stock(f.now, AreaId(2), f.limits.meal_size, empty);
        assert_eq!(m.last_failure, Some((f.now, InventoryId(9))));
        m.relationships.get_mut(&PersonId(2)).unwrap().trust = -501;
        let before = m.clone();
        assert!(!m.receive_report(PersonId(2), f.now, stock(9, Good::Food, 2, 1_000)));
        assert_eq!(m, before);
    }
    #[test]
    fn failed_plan_has_cooldown_and_overflow_is_inert() {
        let mut m = mind();
        let mut f = facts();
        let work = work(&mut m);
        let d = choose(&m, &f, std::slice::from_ref(&work));
        m.record_attempt(f.now, d.plan.unwrap(), d.explanation, false)
            .unwrap();
        assert_eq!(m.retry_after, SimTime(120_100));
        assert_eq!(
            m.explanation.blocker.as_deref(),
            Some("physical attempt failed")
        );
        assert!(choose(&m, &f, std::slice::from_ref(&work)).plan.is_none());
        f.now = m.retry_after;
        let d = choose(&m, &f, &[work]);
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
        let mut f = facts();
        let work = work(&mut m);
        f.activity = Activity::Committed;
        assert!(choose(&m, &f, std::slice::from_ref(&work)).plan.is_none());
        f.activity = Activity::Working;
        m.explanation.total = 10_000;
        assert!(choose(&m, &f, std::slice::from_ref(&work)).plan.is_none());
        m.knowledge
            .insert(InventoryId(9), stock(9, Good::Food, 1, 1_000));
        f.vitals.hunger = 10_000;
        assert_eq!(
            choose(
                &m,
                &f,
                &[
                    candidate(
                        9,
                        Purpose::Food(InventoryId(9)),
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
            choose(&m, &f, &[candidate(1, Purpose::Rest, ActionShape::Rest)])
                .plan
                .is_none()
        );
    }
    #[test]
    fn hydration_requires_observed_contact_and_personal_care_motivation() {
        let mut m = mind();
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
        m.knowledge
            .insert(InventoryId(9), stock(9, Good::Water, 1, 1_000));
        let care = candidate(
            9,
            Purpose::Care {
                child,
                source: InventoryId(9),
            },
            ActionShape::Care(child),
        );
        assert!(choose(&m, &f, std::slice::from_ref(&care)).plan.is_none());
        f.observed_contacts.insert(child);
        assert!(choose(&m, &f, std::slice::from_ref(&care)).plan.is_none());
        m.relationships.insert(
            child,
            Relationship {
                trust: 0,
                affection: 100,
            },
        );
        f.activity = Activity::Working;
        assert_eq!(
            choose(&m, &f, std::slice::from_ref(&care))
                .plan
                .unwrap()
                .action,
            9
        );
        m.child_signals.get_mut(&child).unwrap().observed_at = SimTime(0);
        assert!(choose(&m, &f, &[care]).plan.is_none());
    }
    #[test]
    fn road_choices_stop_before_care_and_do_not_use_origin_warehouses() {
        let mut m = mind();
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
        m.knowledge
            .insert(InventoryId(9), stock(9, Good::Water, 1, 1_000));
        let care = candidate(
            9,
            Purpose::Care {
                child,
                source: InventoryId(9),
            },
            ActionShape::Care(child),
        );
        let stop = candidate(1, Purpose::Stop, ActionShape::Other);
        f.place = Place::Moving;
        assert!(choose(&m, &f, std::slice::from_ref(&care)).plan.is_none());
        assert_eq!(
            choose(&m, &f, &[care.clone(), stop]).plan.unwrap().action,
            1
        );
        f.place = Place::Stopped;
        assert!(choose(&m, &f, std::slice::from_ref(&care)).plan.is_none());
        f.own_cargo.insert(InventoryId(9));
        assert_eq!(choose(&m, &f, &[care]).plan.unwrap().action, 9);
        f.can_resume = true;
        let resume = candidate(3, Purpose::Resume, ActionShape::Other);
        assert!(choose(&m, &f, std::slice::from_ref(&resume)).plan.is_none());
        m.child_signals.clear();
        assert_eq!(choose(&m, &f, &[resume]).plan.unwrap().action, 3);
    }
    #[test]
    fn complete_personal_planning_state_roundtrips_without_world_or_api_types() {
        let mut m = mind();
        let f = facts();
        let work = work(&mut m);
        let d = choose(&m, &f, &[work]);
        m.record_attempt(f.now, d.plan.unwrap(), d.explanation, true)
            .unwrap();
        let restored: Mind<u8> = serde_json::from_slice(&serde_json::to_vec(&m).unwrap()).unwrap();
        assert_eq!(m, restored);
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
