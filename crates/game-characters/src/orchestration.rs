//! Production review orchestration over factual ports and opaque executable actions.
//! Ports read foreign owners or request their operations; all personal motives,
//! candidate templates, evidence interpretation and selection live here.
use crate::decisions::*;
use game_core::*;
use game_ecs::{Component, Store};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug)]
pub enum ActionRequest {
    Eat(InventoryId),
    Move(AreaId),
    Work(SiteId),
    Care(PersonId, InventoryId),
    Comfort(PersonId),
    Rest,
    Play(ChildhoodInterest),
    Stop,
    Resume,
}
#[derive(Clone, Copy, Debug)]
pub enum ActionKind {
    Shape(ActionShape),
    Drink(InventoryId),
    Boil(InventoryId),
    Shelter,
    Project,
    Practice,
    Conditional,
    Other,
}
#[derive(Clone, Debug)]
pub struct ActionEvidence<A> {
    pub action: A,
    pub kind: ActionKind,
    pub risk: Risk,
    pub source: Option<InventoryId>,
}
#[derive(Clone, Debug)]
pub struct ProjectOpportunity<A> {
    pub action: ActionEvidence<A>,
    pub project: Option<ProjectId>,
    pub beneficiaries: Vec<PersonId>,
    pub obligation: Option<ObligationId>,
}
#[derive(Clone, Debug)]
pub struct Opportunities<A> {
    pub health: Option<ActionEvidence<A>>,
    pub provisions: Vec<(ActionEvidence<A>, game_logistics::Requirement)>,
    pub projects: Vec<ProjectOpportunity<A>>,
    pub property: Vec<(ActionEvidence<A>, InventoryId)>,
    pub cultivation: Option<ActionEvidence<A>>,
}
impl<A> Default for Opportunities<A> {
    fn default() -> Self {
        Self {
            health: None,
            provisions: vec![],
            projects: vec![],
            property: vec![],
            cultivation: None,
        }
    }
}
/// Factual public-command binding. These methods carry no character preference,
/// motive, score, selection threshold or review timing.
pub trait DecisionPort<A> {
    fn facts(&self, actor: PersonId) -> PersonalFacts;
    fn action(&self, request: ActionRequest) -> ActionEvidence<A>;
    fn force_action(&self, actor: PersonId) -> Option<ActionEvidence<A>>;
    fn opportunities(&self, actor: PersonId) -> Opportunities<A>;
    fn delivery_action(&self, actor: PersonId, goal: &CareDelivery) -> Option<ActionEvidence<A>>;
}
pub type ReviewJob<'a, A> = (PersonId, (PersonId, &'a Mind<A>));
#[derive(Clone, Debug)]
pub struct Intention<A> {
    pub actor: PersonId,
    pub decision: Decision<A>,
}

fn provision_need(good: Good) -> Option<NeedRef> {
    match good {
        Good::Food => Some(NeedRef::Hunger),
        Good::Water => Some(NeedRef::Hydration),
        _ => None,
    }
}
impl Runtime {
    /// Stable keyed work borrowed directly from canonical personal components.
    pub fn review_jobs<'a, A: Component>(&self, store: &'a Store) -> Vec<ReviewJob<'a, A>> {
        minds::<A>(store)
            .map(|(actor, mind)| (actor, (actor, mind)))
            .collect()
    }
    pub fn decide_from_port<A: Clone>(
        &self,
        actor: PersonId,
        mind: &Mind<A>,
        stocks: &Stocks,
        port: &impl DecisionPort<A>,
    ) -> Intention<A> {
        let facts = port.facts(actor);
        let mut candidates = Vec::new();
        if mind.needs_candidates(&facts) {
            let mut add = |evidence, key, motives, delivery| {
                if let Some(candidate) = candidate(mind, &facts, evidence, key, motives, delivery) {
                    candidates.push(candidate);
                }
            };
            for goal in mind.care_deliveries(&facts) {
                let game_logistics::Destination::Contact(child) = goal.delivery.destination else {
                    continue;
                };
                if let Some(action) = port.delivery_action(actor, &goal) {
                    let good = goal.delivery.requirement.good;
                    add(
                        action,
                        IntentionKey::Provision {
                            good,
                            beneficiary: child,
                            source: goal.delivery.requirement.source,
                        },
                        vec![Motive::Provide {
                            requirement: goal.delivery.requirement.clone(),
                            beneficiary: child,
                            need: Some(if good == Good::Food {
                                NeedRef::Hunger
                            } else {
                                NeedRef::Hydration
                            }),
                        }],
                        Some(goal.delivery),
                    );
                }
            }
            if let Some(action) = port.force_action(actor) {
                add_relief(&mut add, action, NeedRef::Freedom, actor);
            }
            let opportunities = if matches!(facts.place, Place::Area(_)) {
                port.opportunities(actor)
            } else {
                Opportunities::default()
            };
            if matches!(facts.place, Place::Area(_)) {
                if let Some(action) = opportunities.health {
                    let need = if matches!(action.kind, ActionKind::Shelter) {
                        NeedRef::Exposure
                    } else {
                        NeedRef::Hydration
                    };
                    add_relief(&mut add, action, need, actor);
                }
                for (action, requirement) in opportunities.provisions {
                    add(
                        action,
                        IntentionKey::Provision {
                            good: requirement.good,
                            beneficiary: actor,
                            source: None,
                        },
                        vec![Motive::Provide {
                            need: provision_need(requirement.good),
                            requirement,
                            beneficiary: actor,
                        }],
                        None,
                    );
                }
                for opportunity in opportunities.projects {
                    let mut key = IntentionKey::Development {
                        capability: Capability::Livelihood,
                        beneficiary: actor,
                        subject: opportunity.project.map(Subject::Project),
                    };
                    let beneficiaries = if opportunity.project.is_some() {
                        opportunity.beneficiaries
                    } else {
                        vec![actor]
                    };
                    let mut motives = beneficiaries
                        .into_iter()
                        .map(|beneficiary| Motive::Develop {
                            capability: Capability::Livelihood,
                            beneficiary,
                        })
                        .collect::<Vec<_>>();
                    if let Some(obligation) = opportunity.obligation {
                        let commitment = CommitmentRef::Obligation(obligation);
                        motives.push(Motive::Fulfil { commitment });
                        key = IntentionKey::Commitment(commitment);
                    }
                    add(opportunity.action, key, motives, None);
                }
                for (action, inventory) in opportunities.property {
                    let Some(stock) = stocks.known.get(&inventory) else {
                        continue;
                    };
                    let requirement = game_logistics::Requirement {
                        good: stock.good,
                        quantity: facts.limits.meal_size,
                        source: None,
                    };
                    add(
                        action,
                        IntentionKey::Provision {
                            good: stock.good,
                            beneficiary: actor,
                            source: None,
                        },
                        vec![Motive::Provide {
                            requirement,
                            beneficiary: actor,
                            need: provision_need(stock.good),
                        }],
                        None,
                    );
                }
            }
            for stock in stocks
                .known
                .values()
                .filter(|stock| stock.good == Good::Food)
            {
                let request = match facts.place {
                    Place::Area(area) if area != stock.area => ActionRequest::Move(stock.area),
                    _ => ActionRequest::Eat(stock.inventory),
                };
                let mut action = port.action(request);
                action.source = Some(stock.inventory);
                add_relief(&mut add, action, NeedRef::Hunger, actor);
            }
            for child in &facts.observed_contacts {
                add_relief(
                    &mut add,
                    port.action(ActionRequest::Comfort(*child)),
                    NeedRef::Company,
                    *child,
                );
            }
            add_relief(
                &mut add,
                port.action(ActionRequest::Rest),
                NeedRef::Recovery,
                actor,
            );
            if matches!(facts.place, Place::Area(_)) {
                if let Some(action) = opportunities.cultivation {
                    add_development(
                        &mut add,
                        action,
                        Capability::Skill(Skill::Farming),
                        actor,
                        None,
                    );
                }
                for interest in [
                    ChildhoodInterest::Food,
                    ChildhoodInterest::Company,
                    ChildhoodInterest::Movement,
                    ChildhoodInterest::Craft,
                ] {
                    add_development(
                        &mut add,
                        port.action(ActionRequest::Play(interest)),
                        Capability::Interest(interest),
                        actor,
                        None,
                    );
                }
                for site in mind.sites.values() {
                    add_development(
                        &mut add,
                        port.action(ActionRequest::Work(site.site)),
                        Capability::Skill(Skill::Craft),
                        actor,
                        Some(Subject::Site(site.site)),
                    );
                }
            } else {
                for request in [ActionRequest::Stop, ActionRequest::Resume] {
                    add_development(
                        &mut add,
                        port.action(request),
                        Capability::Movement,
                        actor,
                        None,
                    );
                }
            }
        }
        Intention {
            actor,
            decision: mind.decide(
                &facts,
                stocks,
                &candidates,
                self.work_concept,
                self.switching_margin,
            ),
        }
    }
    pub fn learn_production_events<A: Component>(
        &mut self,
        store: &mut Store,
        events: impl IntoIterator<Item = (u64, PersonId)>,
        through: u64,
    ) {
        for (sequence, actor) in events {
            if sequence > self.last_learning_event {
                self.learn_production::<A>(store, actor);
            }
        }
        self.last_learning_event = through;
    }
}
fn add_relief<A>(
    add: &mut impl FnMut(ActionEvidence<A>, IntentionKey, Vec<Motive>, Option<game_logistics::Delivery>),
    action: ActionEvidence<A>,
    need: NeedRef,
    beneficiary: PersonId,
) {
    add(
        action,
        IntentionKey::Relief { need, beneficiary },
        vec![Motive::Relieve { need, beneficiary }],
        None,
    );
}
fn add_development<A>(
    add: &mut impl FnMut(ActionEvidence<A>, IntentionKey, Vec<Motive>, Option<game_logistics::Delivery>),
    action: ActionEvidence<A>,
    capability: Capability,
    beneficiary: PersonId,
    subject: Option<Subject>,
) {
    add(
        action,
        IntentionKey::Development {
            capability,
            beneficiary,
            subject,
        },
        vec![Motive::Develop {
            capability,
            beneficiary,
        }],
        None,
    );
}
fn candidate<A>(
    mind: &Mind<A>,
    facts: &PersonalFacts,
    evidence: ActionEvidence<A>,
    key: IntentionKey,
    motives: Vec<Motive>,
    delivery: Option<game_logistics::Delivery>,
) -> Option<Candidate<A>> {
    let mut prospects = Prospects {
        risk: evidence.risk,
        source: evidence.source,
        ..Default::default()
    };
    let shape = match evidence.kind {
        ActionKind::Shape(shape) => shape,
        _ => ActionShape::Other,
    };
    match evidence.kind {
        ActionKind::Shape(ActionShape::Move(destination)) => {
            if let Place::Area(area) = facts.place {
                prospects.travel = known_path(&mind.routes, area, destination).ok()?.1;
            }
            prospects.uncertainty = Uncertainty::Remembered;
        }
        ActionKind::Shape(ActionShape::Eat(source)) => {
            prospects.source = Some(source);
            prospects.duration = facts.limits.eating_time;
        }
        ActionKind::Shape(ActionShape::Rest) => prospects.effort = Effort::Restoration,
        ActionKind::Shape(ActionShape::Work(site)) => {
            prospects.site = Some(site);
            prospects.effort = Effort::Practice;
        }
        ActionKind::Drink(source) | ActionKind::Boil(source) => prospects.source = Some(source),
        ActionKind::Project => {
            prospects.effort = Effort::Practice;
            prospects.uncertainty = Uncertainty::Conditional;
        }
        ActionKind::Conditional => prospects.uncertainty = Uncertainty::Conditional,
        ActionKind::Practice => prospects.effort = Effort::Practice,
        ActionKind::Other => {}
        ActionKind::Shelter | ActionKind::Shape(_) => {}
    }
    Some(Candidate {
        action: evidence.action,
        key,
        motives,
        prospects,
        shape,
        delivery,
    })
}

#[derive(Clone, Copy, Debug)]
pub struct AgencyFacts {
    pub manual: bool,
    pub in_combat: bool,
    pub family_busy: bool,
    pub mission_busy: bool,
    pub health_busy: bool,
    pub transport_busy: bool,
    pub army_talking: bool,
    pub realm_talking: bool,
    pub property_talking: bool,
}
impl AgencyFacts {
    pub fn control(self) -> Control {
        if self.manual {
            Control::Manual
        } else if self.in_combat
            || self.family_busy
            || self.mission_busy
            || self.health_busy
            || self.transport_busy
        {
            Control::Delegated
        } else {
            Control::Autonomous
        }
    }
    pub fn in_dialogue(self) -> bool {
        self.army_talking || self.realm_talking || self.property_talking
    }
}
pub fn observed_contacts<A>(
    mind: &Mind<A>,
    now: SimTime,
    co_present: impl Fn(&Location, SimTime) -> bool,
) -> BTreeSet<PersonId> {
    mind.child_signals
        .iter()
        .filter(|(_, signal)| {
            signal.observed_at == now && co_present(&signal.location, signal.observed_at)
        })
        .map(|(child, _)| *child)
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ObservationPoint {
    Area(AreaId),
    Road {
        from: AreaId,
        to: AreaId,
        numerator: u64,
        denominator: u64,
    },
}
#[derive(Clone, Debug)]
pub struct ObserverFacts {
    pub actor: PersonId,
    pub location: Location,
    pub point: Option<ObservationPoint>,
    pub alive: bool,
    pub health: u64,
    pub dependent: bool,
    pub meal_size: Quantity,
}
#[derive(Clone, Debug)]
pub struct DependentFacts {
    pub hunger: u64,
    pub meal_relief: u64,
    pub hunger_harm: u64,
    pub preference: Option<(ChildhoodInterest, SimTime)>,
    pub hydration_urgency: i64,
}
#[derive(Clone, Debug)]
pub struct ObservedSite {
    pub site: SiteId,
    pub area: AreaId,
    pub input: InventoryId,
    pub tool: InventoryId,
    pub input_quantity: Quantity,
    pub output: InventoryId,
    pub access: BTreeSet<PersonId>,
}
/// Port facts are physical observations, access rights and local catalogs.
pub trait ObservationPort {
    fn actors(&self) -> Vec<PersonId>;
    fn observer(&self, actor: PersonId) -> ObserverFacts;
    fn co_present(&self, first: &Location, second: &Location, now: SimTime) -> bool;
    fn bears(&self, actor: PersonId, child: PersonId) -> bool;
    fn signal_visible(&self, actor: PersonId, child: PersonId) -> bool;
    fn dependent(&self, child: PersonId) -> DependentFacts;
    fn stocks(
        &self,
        actor: PersonId,
        area: AreaId,
        at_area: bool,
        now: SimTime,
    ) -> Vec<StockKnowledge>;
    fn sites(&self) -> Vec<ObservedSite>;
    fn routes(&self, area: AreaId) -> BTreeMap<AreaId, SimDuration>;
}
pub struct Observation {
    actor: PersonId,
    location: Location,
    area: AreaId,
    meal_size: Quantity,
    routes: Option<BTreeMap<AreaId, SimDuration>>,
    stocks: Vec<StockKnowledge>,
    children: Vec<(PersonId, ChildSignal)>,
    sites: Vec<SiteKnowledge>,
}
/// Cohort joining and bounded dependent attention are receiving character policy.
pub fn prepare_observations(now: SimTime, port: &impl ObservationPort) -> Vec<Observation> {
    let mut actors = port
        .actors()
        .into_iter()
        .map(|actor| port.observer(actor))
        .collect::<Vec<_>>();
    actors.sort_by_key(|actor| actor.actor);
    // Exact factual point keys keep dependent attention local and bounded.
    let mut cohorts: BTreeMap<ObservationPoint, Vec<&ObserverFacts>> = BTreeMap::new();
    let mut signals = BTreeMap::new();
    for child in &actors {
        if child.dependent
            && child.alive
            && let Some(cell) = child.point
        {
            cohorts.entry(cell).or_default().push(child);
            signals.insert(child.actor, port.dependent(child.actor));
        }
    }
    let mut sites: BTreeMap<AreaId, Vec<ObservedSite>> = BTreeMap::new();
    for site in port.sites() {
        sites.entry(site.area).or_default().push(site);
    }
    let mut observations = Vec::new();
    let mut routes = BTreeMap::new();
    for actor in &actors {
        if !actor.alive || actor.health == 0 {
            continue;
        }
        let Some(area) = actor.location.cell() else {
            continue;
        };
        let at_area = matches!(actor.location, Location::AtArea(_));
        let stationary = matches!(
            actor.location,
            Location::AtArea(_) | Location::StoppedOnRoad { .. }
        );
        let children = actor
            .point
            .and_then(|point| cohorts.get(&point))
            .into_iter()
            .flatten()
            .filter(|child| port.co_present(&actor.location, &child.location, now))
            .filter(|child| stationary || port.bears(actor.actor, child.actor))
            .filter(|child| port.signal_visible(actor.actor, child.actor))
            .take(64)
            .map(|child| {
                let facts = &signals[&child.actor];
                let preference = facts
                    .preference
                    .filter(|(_, until)| *until >= now)
                    .map(|(interest, _)| interest);
                let mut urgency = if facts.hunger >= facts.hunger_harm {
                    20_000
                } else if facts.hunger >= facts.meal_relief {
                    3_000
                } else {
                    0
                };
                if preference == Some(ChildhoodInterest::Food) {
                    urgency = urgency.max(3_000);
                }
                (
                    child.actor,
                    ChildSignal {
                        preference,
                        location: child.location.clone(),
                        urgency,
                        hydration_urgency: facts.hydration_urgency,
                        observed_at: now,
                    },
                )
            })
            .collect();
        observations.push(Observation {
            actor: actor.actor,
            location: actor.location.clone(),
            area,
            meal_size: actor.meal_size,
            routes: at_area.then(|| {
                routes
                    .entry(area)
                    .or_insert_with(|| port.routes(area))
                    .clone()
            }),
            stocks: port.stocks(actor.actor, area, at_area, now),
            children,
            sites: if at_area {
                sites
                    .get(&area)
                    .into_iter()
                    .flatten()
                    .map(|site| SiteKnowledge {
                        site: site.site,
                        area,
                        input: site.input,
                        tool: site.tool,
                        input_quantity: site.input_quantity,
                        output: site.output,
                        access: site.access.contains(&actor.actor),
                    })
                    .collect()
            } else {
                vec![]
            },
        });
    }
    observations
}
pub fn apply_observations<A: Component>(
    store: &mut Store,
    now: SimTime,
    observations: Vec<Observation>,
) -> Result<(), Error> {
    for observation in observations {
        for fact in observation.stocks {
            observe_stock::<A>(
                store,
                observation.actor,
                now,
                observation.area,
                observation.meal_size,
                fact,
            )?;
        }
        let mind = mind_mut::<A>(store, observation.actor).ok_or(Error::Unavailable)?;
        if let Some(routes) = observation.routes {
            mind.routes.insert(observation.area, routes);
        }
        mind.child_signals
            .retain(|_, signal| signal.location != observation.location);
        for (child, signal) in observation.children {
            mind.child_signals.insert(child, signal);
        }
        mind.reconcile_care_observation(observation.actor);
        for site in observation.sites {
            mind.sites.insert(site.site, site);
        }
    }
    Ok(())
}

/// Birth creates a fresh personal mind from the authored inherited disposition.
/// It clears all learned beliefs and plans and applies colocated parental affection.
pub fn prepare_newborn_mind<A: Component>(
    store: &Store,
    child: PersonId,
    gestator: PersonId,
    parents: [PersonId; 2],
    now: SimTime,
    same_place_parents: &[PersonId],
) -> Result<PreparedNewbornMind<A>, Error> {
    if mind::<A>(store, child).is_some() {
        return Err(Error::Unavailable);
    }
    let before = mind::<A>(store, gestator)
        .cloned()
        .ok_or(Error::Unavailable)?;
    let mut newborn = before.clone();
    newborn.interests.clear();
    newborn.routes.clear();
    newborn.last_failure = None;
    newborn.child_signals.clear();
    newborn.sites.clear();
    newborn.values.clear();
    newborn.relationships.clear();
    newborn.dream = Aspiration::Comfort;
    newborn.plan = None;
    newborn.retry_after = now;
    newborn.goal = Goal::Survival;
    newborn.explanation = Explanation {
        goal: Goal::Survival,
        need: 0,
        benefit: 0,
        value: 0,
        total: 0,
        blocker: None,
    };
    let mut changes = Vec::new();
    for parent in parents.into_iter().collect::<BTreeSet<_>>() {
        if !same_place_parents.contains(&parent) {
            continue;
        }
        if let Some(row) = mind::<A>(store, parent) {
            let mut after = row.clone();
            after.relationships.insert(
                child,
                Relationship {
                    trust: 0,
                    affection: i32::from(row.personality.empathy),
                },
            );
            changes.push((parent, row.clone(), after));
        }
    }
    Ok(PreparedNewbornMind {
        child,
        gestator,
        before,
        newborn,
        changes,
        entity_present: store.contains(game_ecs::StableId(child.0)),
    })
}
pub struct PreparedNewbornMind<A> {
    child: PersonId,
    gestator: PersonId,
    before: Mind<A>,
    newborn: Mind<A>,
    changes: Vec<(PersonId, Mind<A>, Mind<A>)>,
    entity_present: bool,
}
impl<A: Component> PreparedNewbornMind<A> {
    pub fn personality(&self) -> &Personality {
        &self.newborn.personality
    }
    pub fn with_personality(mut self, personality: Personality) -> Result<Self, Error> {
        if !personality.valid() {
            return Err(Error::InvalidConfiguration(
                "invalid newborn personality".into(),
            ));
        }
        self.newborn.personality = personality;
        Ok(self)
    }
    pub fn stage_into(
        self,
        store: &Store,
        batch: &mut game_ecs::Batch,
    ) -> Result<game_ecs::ReadGuard, Error> {
        let mut reads = game_ecs::ReadSet::new();
        let child = game_ecs::StableId(self.child.0);
        if store.contains(child) != self.entity_present {
            return Err(Error::Unavailable);
        }
        reads.entity(store, child);
        reads.expect_row::<Mind<A>>(child, None);
        reads.expect_row::<Stocks>(child, None);
        reads.expect_row(game_ecs::StableId(self.gestator.0), Some(self.before));
        for (actor, before, _) in &self.changes {
            reads.expect_row(game_ecs::StableId(actor.0), Some(before.clone()));
        }
        reads.validate(store)?;
        if batch.touches::<Mind<A>>(child)
            || batch.touches::<Stocks>(child)
            || self
                .changes
                .iter()
                .any(|(actor, _, _)| batch.touches::<Mind<A>>(game_ecs::StableId(actor.0)))
        {
            return Err(Error::Unavailable);
        }
        self.newborn.validate()?;
        let mut admitted = game_ecs::ReadFootprint::new();
        admitted.protect::<Mind<A>>(child);
        admitted.protect::<Stocks>(child);
        batch.insert(child, self.newborn);
        batch.insert(child, Stocks::default());
        for (actor, _, after) in self.changes {
            after.validate()?;
            admitted.protect::<Mind<A>>(game_ecs::StableId(actor.0));
            batch.insert(game_ecs::StableId(actor.0), after);
        }
        reads.bind(store, batch, &admitted)
    }
    pub fn commit(self, store: &mut Store) -> Result<(), Error> {
        let mut batch = game_ecs::Batch::new();
        let guard = self.stage_into(store, &mut batch)?;
        guard.validate(store, &batch)?;
        store.apply_batch(batch)
    }
}

/// Record the execution result through the receiving owner, preserving its retry
/// clock and plan reconciliation even when the physical command failed.
pub fn record_attempt<A: Component>(
    store: &mut Store,
    actor: PersonId,
    now: SimTime,
    plan: Plan<A>,
    explanation: Explanation,
    accepted: bool,
) -> Result<(), Error> {
    mind_mut::<A>(store, actor)
        .ok_or(Error::Unavailable)?
        .record_attempt(now, plan, explanation, accepted)
}
pub fn record_care_receipt<A: Component>(
    store: &mut Store,
    caregiver: PersonId,
    child: PersonId,
    good: Good,
    quantity: Quantity,
    sequence: u64,
) -> Result<bool, Error> {
    match mind_mut::<A>(store, caregiver) {
        Some(mind) => mind.record_care_receipt(child, good, quantity, sequence),
        None => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planning::tests::{facts, mind};
    use std::cell::Cell;
    struct Port {
        facts: PersonalFacts,
        opportunities: Cell<usize>,
    }
    impl DecisionPort<u8> for Port {
        fn facts(&self, _: PersonId) -> PersonalFacts {
            self.facts.clone()
        }
        fn action(&self, request: ActionRequest) -> ActionEvidence<u8> {
            let (action, kind) = match request {
                ActionRequest::Eat(source) => (1, ActionKind::Shape(ActionShape::Eat(source))),
                ActionRequest::Move(area) => (2, ActionKind::Shape(ActionShape::Move(area))),
                ActionRequest::Rest => (3, ActionKind::Shape(ActionShape::Rest)),
                ActionRequest::Play(_) => (4, ActionKind::Practice),
                _ => (5, ActionKind::Other),
            };
            ActionEvidence {
                action,
                kind,
                risk: Risk::Safe,
                source: None,
            }
        }
        fn force_action(&self, _: PersonId) -> Option<ActionEvidence<u8>> {
            None
        }
        fn opportunities(&self, _: PersonId) -> Opportunities<u8> {
            self.opportunities.set(self.opportunities.get() + 1);
            Opportunities::default()
        }
        fn delivery_action(&self, _: PersonId, _: &CareDelivery) -> Option<ActionEvidence<u8>> {
            None
        }
    }
    fn runtime() -> Runtime {
        Runtime {
            concepts: BTreeMap::from([(ConceptId(1), Good::Food)]),
            work_concept: ConceptId(1),
            last_learning_event: 0,
            switching_margin: 50,
        }
    }
    fn known(area: AreaId) -> Stocks {
        Stocks {
            known: BTreeMap::from([(
                InventoryId(10),
                StockKnowledge {
                    inventory: InventoryId(10),
                    area,
                    good: Good::Food,
                    quantity: Quantity(1000),
                    accessible: true,
                    observed_at: SimTime(100),
                    learned_at: SimTime(100),
                    source: KnowledgeSource::Observed(PersonId(1)),
                },
            )]),
        }
    }
    #[test]
    fn production_review_templates_require_known_routes_and_do_not_read_unseen_stocks() {
        let mut mind = mind();
        let mut facts = facts();
        facts.vitals.hunger = facts.limits.hunger_harm;
        let port = Port {
            facts,
            opportunities: Cell::new(0),
        };
        let decision = runtime()
            .decide_from_port(PersonId(1), &mind, &known(AreaId(1)), &port)
            .decision;
        assert_eq!(decision.plan.unwrap().action, 1);
        assert_eq!(
            runtime()
                .decide_from_port(PersonId(1), &mind, &known(AreaId(2)), &port)
                .decision
                .plan
                .unwrap()
                .action,
            3
        );
        mind.routes
            .insert(AreaId(1), BTreeMap::from([(AreaId(2), SimDuration(20))]));
        let plan = runtime()
            .decide_from_port(PersonId(1), &mind, &known(AreaId(2)), &port)
            .decision
            .plan
            .unwrap();
        assert_eq!(plan.action, 2);
        assert_eq!(plan.prospects.travel, SimDuration(20));
        assert_eq!(plan.prospects.source, Some(InventoryId(10)));
        assert_eq!(port.opportunities.get(), 3);
    }
    #[test]
    fn inactive_review_never_invokes_foreign_opportunities_and_failures_keep_owner_retry_clock() {
        let mut facts = facts();
        facts.control = Control::Manual;
        let mut port = Port {
            facts,
            opportunities: Cell::new(0),
        };
        let mut mind = mind();
        assert!(
            runtime()
                .decide_from_port(PersonId(1), &mind, &known(AreaId(1)), &port)
                .decision
                .plan
                .is_none()
        );
        assert_eq!(port.opportunities.get(), 0);
        port.facts.control = Control::Autonomous;
        port.facts.vitals.hunger = port.facts.limits.hunger_harm;
        let decision = runtime()
            .decide_from_port(PersonId(1), &mind, &known(AreaId(1)), &port)
            .decision;
        mind.record_attempt(
            port.facts.now,
            decision.plan.unwrap(),
            decision.explanation,
            false,
        )
        .unwrap();
        port.facts.now = SimTime(port.facts.now.0 + 1);
        assert_eq!(mind.retry_after, SimTime(120_100));
        assert!(mind.explanation.blocker.is_some());
        assert_eq!(port.opportunities.get(), 1);
    }
    #[test]
    fn delivery_forecasts_refresh_local_observations_and_preserve_distant_dates() {
        let known = known(AreaId(2));
        let known = &known.known[&InventoryId(10)];
        let remote =
            delivery_known_stock(known, &Location::AtArea(AreaId(1)), None, SimTime(200)).unwrap();
        assert_eq!(remote.quantity, Quantity(1000));
        assert_eq!(remote.observed_at, SimTime(100));
        assert_eq!(remote.room, None);
        let absent =
            delivery_known_stock(known, &Location::AtArea(AreaId(2)), None, SimTime(200)).unwrap();
        assert_eq!(absent.quantity, Quantity(0));
        assert_eq!(absent.observed_at, SimTime(200));
        let local = LocalStockFacts {
            location: Location::AtArea(AreaId(3)),
            quantity: Quantity(20),
            room: Quantity(80),
            accessible: false,
        };
        let observed = delivery_known_stock(
            known,
            &Location::AtArea(AreaId(3)),
            Some(local.clone()),
            SimTime(200),
        )
        .unwrap();
        assert_eq!(observed.location, Location::AtArea(AreaId(3)));
        assert_eq!(observed.quantity, Quantity(20));
        assert!(!observed.accessible);
        let destination =
            delivery_known_destination(known.inventory, Some(known), None, SimTime(200)).unwrap();
        assert_eq!(destination.location, Location::AtArea(AreaId(2)));
        assert_eq!(destination.room, None);
        let destination =
            delivery_known_destination(known.inventory, None, Some(local), SimTime(200)).unwrap();
        assert_eq!(destination.room, Some(Quantity(80)));
        assert_eq!(destination.observed_at, SimTime(200));
        assert!(
            delivery_known_stock(known, &Location::AtArea(AreaId(1)), None, SimTime(99)).is_none()
        );
        assert!(
            delivery_known_destination(known.inventory, Some(known), None, SimTime(99)).is_none()
        );
    }

    struct Observations;
    impl ObservationPort for Observations {
        fn actors(&self) -> Vec<PersonId> {
            (1..=67).rev().map(PersonId).collect()
        }
        fn observer(&self, actor: PersonId) -> ObserverFacts {
            ObserverFacts {
                actor,
                location: Location::AtArea(AreaId(1)),
                point: Some(ObservationPoint::Area(AreaId(1))),
                alive: true,
                health: 1000,
                dependent: actor != PersonId(1),
                meal_size: Quantity(1000),
            }
        }
        fn co_present(&self, a: &Location, b: &Location, _: SimTime) -> bool {
            a == b
        }
        fn bears(&self, _: PersonId, _: PersonId) -> bool {
            false
        }
        fn signal_visible(&self, _: PersonId, _: PersonId) -> bool {
            true
        }
        fn dependent(&self, actor: PersonId) -> DependentFacts {
            DependentFacts {
                hunger: if actor == PersonId(2) { 20000 } else { 0 },
                meal_relief: 10000,
                hunger_harm: 20000,
                preference: Some((ChildhoodInterest::Food, SimTime(100))),
                hydration_urgency: 0,
            }
        }
        fn stocks(&self, _: PersonId, _: AreaId, _: bool, _: SimTime) -> Vec<StockKnowledge> {
            vec![]
        }
        fn sites(&self) -> Vec<ObservedSite> {
            vec![]
        }
        fn routes(&self, _: AreaId) -> BTreeMap<AreaId, SimDuration> {
            BTreeMap::from([(AreaId(2), SimDuration(20))])
        }
    }
    #[test]
    fn production_perception_orders_and_bounds_attention_then_updates_owned_memory() {
        let observations = prepare_observations(SimTime(100), &Observations);
        assert_eq!(observations[0].actor, PersonId(1));
        assert_eq!(observations[0].children.len(), 64);
        assert_eq!(observations[0].children[0].0, PersonId(2));
        assert_eq!(observations[0].children[0].1.urgency, 20000);
        assert_eq!(observations[0].children[1].1.urgency, 3000);
        let mut schema = game_ecs::Schema::new();
        register_minds::<u8>(&mut schema).unwrap();
        let mut store = Store::with_schema(schema);
        for actor in 1..=67 {
            insert_mind(&mut store, PersonId(actor), mind()).unwrap();
        }
        apply_observations::<u8>(&mut store, SimTime(100), observations).unwrap();
        let actor = crate::decisions::mind::<u8>(&store, PersonId(1)).unwrap();
        assert_eq!(actor.child_signals.len(), 64);
        assert_eq!(actor.routes[&AreaId(1)][&AreaId(2)], SimDuration(20));
        assert!(!actor.child_signals.contains_key(&PersonId(67)));
        let observations = prepare_observations(SimTime(101), &Observations);
        apply_observations::<u8>(&mut store, SimTime(101), observations).unwrap();
        assert_eq!(
            crate::decisions::mind::<u8>(&store, PersonId(1))
                .unwrap()
                .child_signals[&PersonId(3)]
                .urgency,
            0
        );
    }

    #[test]
    fn newborn_admission_resets_beliefs_and_grants_only_present_parent_affection() {
        let mut schema = game_ecs::Schema::new();
        register_minds::<u8>(&mut schema).unwrap();
        let mut store = Store::with_schema(schema);
        let mut gestator = mind();
        gestator.routes.insert(AreaId(1), BTreeMap::new());
        insert_mind(&mut store, PersonId(1), gestator).unwrap();
        insert_mind(&mut store, PersonId(2), mind()).unwrap();
        prepare_newborn_mind::<u8>(
            &store,
            PersonId(3),
            PersonId(1),
            [PersonId(1), PersonId(2)],
            SimTime(200),
            &[PersonId(1)],
        )
        .unwrap()
        .commit(&mut store)
        .unwrap();
        let child = crate::decisions::mind::<u8>(&store, PersonId(3)).unwrap();
        assert!(child.routes.is_empty());
        assert_eq!(child.dream, Aspiration::Comfort);
        assert_eq!(child.retry_after, SimTime(200));
        assert_eq!(
            crate::decisions::mind::<u8>(&store, PersonId(1))
                .unwrap()
                .relationships[&PersonId(3)]
                .affection,
            500
        );
        assert!(
            !crate::decisions::mind::<u8>(&store, PersonId(2))
                .unwrap()
                .relationships
                .contains_key(&PersonId(3))
        );
    }
}

pub fn apply_relationship_gain_to_mind<A>(
    mind: &mut Mind<A>,
    other: PersonId,
    affection_gain: i32,
    trust_gain: i32,
) {
    let relation = mind.relationships.entry(other).or_insert(Relationship {
        trust: 0,
        affection: 0,
    });
    relation.affection = relation.affection.saturating_add(affection_gain).min(1000);
    relation.trust = relation.trust.saturating_add(trust_gain).min(1000);
}
pub fn apply_relationship_gain<A: Component>(
    store: &mut Store,
    first: PersonId,
    second: PersonId,
    affection_gain: i32,
    trust_gain: i32,
) {
    for (actor, subject) in [(first, second), (second, first)] {
        if let Some(mind) = mind_mut::<A>(store, actor) {
            apply_relationship_gain_to_mind(mind, subject, affection_gain, trust_gain);
        }
    }
}
pub fn learn_routes_to_mind<A>(
    mind: &mut Mind<A>,
    routes: &BTreeMap<AreaId, BTreeMap<AreaId, SimDuration>>,
) {
    for (area, links) in routes {
        mind.routes.entry(*area).or_default().extend(
            links
                .iter()
                .map(|(destination, duration)| (*destination, *duration)),
        );
    }
}
pub fn learn_routes<A: Component>(
    store: &mut Store,
    student: PersonId,
    routes: &BTreeMap<AreaId, BTreeMap<AreaId, SimDuration>>,
) {
    if let Some(mind) = mind_mut::<A>(store, student) {
        learn_routes_to_mind(mind, routes);
    }
}

/// Personal site memories suggest bounded input amounts. A visible destination
/// quantity is a supplied observation, never an unrestricted inventory lookup.
pub fn known_input_quantities<A: Component>(
    store: &Store,
    actor: PersonId,
    good: Good,
    destination: Option<InventoryId>,
    visible_destination_quantity: Option<Quantity>,
) -> Vec<Quantity> {
    let Some(mind) = mind::<A>(store, actor) else {
        return vec![];
    };
    let Some(stocks) = stocks(store, actor) else {
        return vec![];
    };
    mind.sites
        .values()
        .filter(|site| {
            stocks
                .known
                .get(&site.input)
                .is_some_and(|stock| stock.good == good)
        })
        .map(|site| {
            if destination == Some(site.input) {
                Quantity(
                    site.input_quantity
                        .0
                        .saturating_sub(visible_destination_quantity.unwrap_or(Quantity(0)).0),
                )
            } else {
                site.input_quantity
            }
        })
        .collect()
}

pub fn needy_child_present<A: Component>(store: &Store, actor: PersonId, area: AreaId) -> bool {
    mind::<A>(store, actor).is_some_and(|mind| {
        mind.child_signals
            .values()
            .any(|signal| signal.location == Location::AtArea(area) && signal.urgency >= 1000)
    })
}
/// Workshop offers are reconstructed solely from personal site and stock memory.
/// The action binding only translates a site ID into its executable command.
pub fn production_acquisition_offers<A: Component>(
    store: &Store,
    actor: PersonId,
    good: Good,
    carried_output_ids: &BTreeSet<InventoryId>,
    action: impl Fn(SiteId) -> A,
) -> Vec<game_logistics::AcquisitionOffer<A>> {
    let Some(mind) = mind::<A>(store, actor) else {
        return vec![];
    };
    let Some(stocks) = stocks(store, actor) else {
        return vec![];
    };
    mind.sites
        .values()
        .filter_map(|site| {
            let output = stocks.known.get(&site.output)?;
            let input = stocks.known.get(&site.input)?;
            let tool = stocks.known.get(&site.tool)?;
            if output.good != good || !site.access || !output.accessible {
                return None;
            }
            Some(game_logistics::AcquisitionOffer {
                key: site.site.0,
                good,
                source: Some(site.output),
                location: Location::AtArea(site.area),
                observed_at: output
                    .observed_at
                    .max(input.observed_at)
                    .max(tool.observed_at),
                accessible: true,
                quantity: game_logistics::OfferYield::Unknown,
                requires_carrying_room: carried_output_ids.contains(&site.output),
                requirements: vec![
                    game_logistics::Prerequisite {
                        requirement: game_logistics::Requirement {
                            good: input.good,
                            quantity: site.input_quantity,
                            source: None,
                        },
                        placement: game_logistics::PrerequisitePlacement::AccessibleAtProvider {
                            inventory: Some(site.input),
                        },
                    },
                    game_logistics::Prerequisite {
                        requirement: game_logistics::Requirement {
                            good: tool.good,
                            quantity: Quantity(1000),
                            source: None,
                        },
                        placement: game_logistics::PrerequisitePlacement::AccessibleAtProvider {
                            inventory: Some(site.tool),
                        },
                    },
                ],
                action: action(site.site),
            })
        })
        .collect()
}

#[derive(Clone, Debug)]
pub struct LocalStockFacts {
    pub location: Location,
    pub quantity: Quantity,
    pub room: Quantity,
    pub accessible: bool,
}
/// Actual placement and contact facts do not establish what an actor knows.
#[derive(Clone, Copy, Debug)]
pub enum DeliveryStockPlacement {
    Area(AreaId),
    Carried { holder: PersonId, same_place: bool },
}
#[derive(Clone, Debug)]
pub struct CurrentDeliveryStock {
    pub placement: DeliveryStockPlacement,
    pub values: LocalStockFacts,
}
impl CurrentDeliveryStock {
    fn visible(
        &self,
        actor: PersonId,
        location: &Location,
        known: bool,
        destination: bool,
    ) -> bool {
        match self.placement {
            DeliveryStockPlacement::Area(area) => known && *location == Location::AtArea(area),
            DeliveryStockPlacement::Carried { holder, same_place } => {
                (destination && holder == actor) || (known && same_place)
            }
        }
    }
}
pub fn delivery_stock_from_facts(
    known: &StockKnowledge,
    actor: PersonId,
    actor_location: &Location,
    current: Option<CurrentDeliveryStock>,
    now: SimTime,
) -> Option<game_logistics::KnownStock> {
    let local = current
        .filter(|stock| stock.visible(actor, actor_location, true, false))
        .map(|stock| stock.values);
    delivery_known_stock(known, actor_location, local, now)
}
pub fn delivery_destination_from_facts(
    inventory: InventoryId,
    known: Option<&StockKnowledge>,
    actor: PersonId,
    actor_location: &Location,
    current: Option<CurrentDeliveryStock>,
    now: SimTime,
) -> Option<game_logistics::KnownDestination> {
    // A retired inventory cannot be a destination even when remembered.
    let current = current?;
    let local = current
        .visible(actor, actor_location, known.is_some(), true)
        .then_some(current.values);
    delivery_known_destination(inventory, known, local, now)
}

/// Acquisition action facts are independent of any executable command type.
#[derive(Clone, Copy, Debug)]
pub enum AcquisitionKind {
    PropertyTaking,
    Gathering { unauthorized: bool },
    Other,
}
pub fn acquisition_risk(kind: AcquisitionKind) -> Risk {
    match kind {
        AcquisitionKind::PropertyTaking | AcquisitionKind::Gathering { unauthorized: true } => {
            Risk::UnconsentedTaking
        }
        _ => Risk::Safe,
    }
}
pub fn acquisition_candidates<A>(
    personality: &Personality,
    offers: impl IntoIterator<Item = game_logistics::AcquisitionOffer<A>>,
    kind: impl Fn(&A) -> AcquisitionKind,
) -> Vec<game_logistics::Candidate<A>> {
    offers
        .into_iter()
        .map(|offer| game_logistics::Candidate {
            willing: personality.accepts_risk(acquisition_risk(kind(&offer.action))),
            offer,
        })
        .collect()
}
pub fn acquisition_request_quantity(quantity: Quantity, room: Quantity) -> Quantity {
    Quantity(quantity.0.min(room.0))
}
#[derive(Clone, Copy, Debug)]
pub enum ConversationActivity {
    Idle,
    Rest,
    Other,
}
/// Foreign occupancy is factual; Characters owns whether conversation can begin.
#[derive(Clone, Copy, Debug)]
pub struct ConversationAvailability {
    pub physical_enabled: bool,
    pub activity: Option<ConversationActivity>,
    pub army_talking: bool,
    pub mission_talking: bool,
    pub realm_busy: bool,
    pub property_talking: bool,
    pub social_participant: bool,
}
impl ConversationAvailability {
    pub fn available(self) -> bool {
        self.physical_enabled
            && matches!(
                self.activity,
                Some(ConversationActivity::Idle | ConversationActivity::Rest)
            )
            && !self.army_talking
            && !self.mission_talking
            && !self.realm_busy
            && !self.property_talking
            && !self.social_participant
    }
}
/// Refresh only observed local stock. Absent local stock disproves a remembered
/// balance in the current area, while distant memories retain their dated values.
pub fn delivery_known_stock(
    known: &StockKnowledge,
    actor_location: &Location,
    local: Option<LocalStockFacts>,
    now: SimTime,
) -> Option<game_logistics::KnownStock> {
    if known.learned_at > now || known.observed_at > now {
        return None;
    }
    let here = *actor_location == Location::AtArea(known.area);
    Some(game_logistics::KnownStock {
        inventory: known.inventory,
        good: known.good,
        quantity: local
            .as_ref()
            .map_or(if here { Quantity(0) } else { known.quantity }, |stock| {
                stock.quantity
            }),
        accessible: local
            .as_ref()
            .map_or(known.accessible, |stock| stock.accessible),
        location: local
            .as_ref()
            .map_or(Location::AtArea(known.area), |stock| stock.location.clone()),
        observed_at: if local.is_some() || here {
            now
        } else {
            known.observed_at
        },
        room: local.map(|stock| stock.room),
    })
}

pub fn delivery_known_destination(
    inventory: InventoryId,
    known: Option<&StockKnowledge>,
    local: Option<LocalStockFacts>,
    now: SimTime,
) -> Option<game_logistics::KnownDestination> {
    let (location, accessible, observed_at, room) = if let Some(local) = local {
        (local.location, local.accessible, now, Some(local.room))
    } else {
        let known = known.filter(|known| known.learned_at <= now && known.observed_at <= now)?;
        (
            Location::AtArea(known.area),
            known.accessible,
            known.observed_at,
            None,
        )
    };
    Some(game_logistics::KnownDestination {
        destination: game_logistics::Destination::Inventory(inventory),
        location,
        observed_at,
        accessible,
        room,
        direct_transfer: false,
    })
}

/// Sparse facts identifying competing claims. Source/target binding belongs to
/// the command adapter; rotating fairness and stable actor ordering belong here.
#[derive(Clone, Copy, Debug)]
pub struct ClaimFacts {
    pub resource: u64,
    pub last_granted: Option<PersonId>,
}
pub fn order_intentions<A>(
    intentions: &mut [Intention<A>],
    facts: impl Fn(&Intention<A>) -> ClaimFacts,
) {
    intentions.sort_by_key(|intention| {
        let claim = facts(intention);
        (
            claim.resource,
            claim
                .last_granted
                .is_some_and(|last| intention.actor <= last),
            intention.actor,
        )
    });
}
pub const REVIEW_INTERVAL: SimDuration = SimDuration(60_000);
pub const FAILED_RETRY_INTERVAL: SimDuration = SimDuration(120_000);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReviewTiming {
    pub deadline: SimTime,
    pub retry_horizon: SimTime,
}
pub fn review_timing(now: SimTime, autonomy_enabled: bool) -> Result<ReviewTiming, Error> {
    let deadline = SimTime(
        now.0
            .checked_add(REVIEW_INTERVAL.0)
            .ok_or(Error::Overflow)?,
    );
    let retry_horizon = SimTime(
        now.0
            .checked_add(if autonomy_enabled {
                FAILED_RETRY_INTERVAL.0
            } else {
                REVIEW_INTERVAL.0
            })
            .ok_or(Error::Overflow)?,
    );
    Ok(ReviewTiming {
        deadline,
        retry_horizon,
    })
}
pub fn review_due(now: SimTime) -> bool {
    now.0.is_multiple_of(REVIEW_INTERVAL.0)
}

#[cfg(test)]
mod review_protocol_tests {
    use super::*;
    #[test]
    fn rotating_claim_order_groups_resources_then_wraps_after_last_granted() {
        let mut intents = [5, 2, 7, 3].map(|actor| Intention {
            actor: PersonId(actor),
            decision: Decision::<u8> {
                plan: None,
                explanation: Explanation {
                    goal: Goal::Rest,
                    need: 0,
                    benefit: 0,
                    value: 0,
                    total: 0,
                    blocker: None,
                },
            },
        });
        order_intentions(&mut intents, |intention| ClaimFacts {
            resource: if intention.actor == PersonId(7) { 2 } else { 1 },
            last_granted: Some(PersonId(3)),
        });
        assert_eq!(
            intents.map(|intention| intention.actor),
            [PersonId(5), PersonId(2), PersonId(3), PersonId(7)]
        );
    }
    #[test]
    fn review_and_failed_retry_preflight_preserve_exact_fixed_deadlines() {
        assert_eq!(
            review_timing(SimTime(60_000), true).unwrap(),
            ReviewTiming {
                deadline: SimTime(120_000),
                retry_horizon: SimTime(180_000)
            }
        );
        assert_eq!(
            review_timing(SimTime(60_000), false).unwrap(),
            ReviewTiming {
                deadline: SimTime(120_000),
                retry_horizon: SimTime(120_000)
            }
        );
        assert!(review_due(SimTime(120_000)));
        assert!(!review_due(SimTime(120_001)));
        assert!(review_timing(SimTime(u64::MAX - 119_999), true).is_err());
        assert!(review_timing(SimTime(u64::MAX - 119_999), false).is_ok());
    }
}

#[cfg(test)]
mod delivery_perception_tests {
    use super::*;
    #[test]
    fn conversation_requires_physical_idle_or_rest_and_every_foreign_slot_free() {
        let free = ConversationAvailability {
            physical_enabled: true,
            activity: Some(ConversationActivity::Idle),
            army_talking: false,
            mission_talking: false,
            realm_busy: false,
            property_talking: false,
            social_participant: false,
        };
        assert!(free.available());
        assert!(
            ConversationAvailability {
                activity: Some(ConversationActivity::Rest),
                ..free
            }
            .available()
        );
        for activity in [None, Some(ConversationActivity::Other)] {
            assert!(!ConversationAvailability { activity, ..free }.available());
        }
        for activity in [
            Some(ConversationActivity::Idle),
            Some(ConversationActivity::Rest),
        ] {
            assert!(
                !ConversationAvailability {
                    physical_enabled: false,
                    activity,
                    ..free
                }
                .available()
            );
        }
        for occupied in [
            ConversationAvailability {
                army_talking: true,
                ..free
            },
            ConversationAvailability {
                mission_talking: true,
                ..free
            },
            ConversationAvailability {
                realm_busy: true,
                ..free
            },
            ConversationAvailability {
                property_talking: true,
                ..free
            },
            ConversationAvailability {
                social_participant: true,
                ..free
            },
        ] {
            assert!(!occupied.available());
        }
    }
    fn memory() -> StockKnowledge {
        StockKnowledge {
            inventory: InventoryId(10),
            area: AreaId(2),
            good: Good::Food,
            quantity: Quantity(1000),
            accessible: true,
            observed_at: SimTime(100),
            learned_at: SimTime(110),
            source: KnowledgeSource::Reported(PersonId(2)),
        }
    }
    fn current(placement: DeliveryStockPlacement) -> CurrentDeliveryStock {
        CurrentDeliveryStock {
            placement,
            values: LocalStockFacts {
                location: Location::AtArea(AreaId(1)),
                quantity: Quantity(20),
                room: Quantity(80),
                accessible: false,
            },
        }
    }
    #[test]
    fn unknown_ground_and_other_carriers_stay_hidden_but_own_cargo_is_live() {
        let actor = PersonId(1);
        let here = Location::AtArea(AreaId(1));
        for placement in [
            DeliveryStockPlacement::Area(AreaId(1)),
            DeliveryStockPlacement::Carried {
                holder: PersonId(2),
                same_place: true,
            },
        ] {
            assert!(
                delivery_destination_from_facts(
                    InventoryId(10),
                    None,
                    actor,
                    &here,
                    Some(current(placement)),
                    SimTime(200)
                )
                .is_none()
            );
        }
        let own = delivery_destination_from_facts(
            InventoryId(10),
            None,
            actor,
            &here,
            Some(current(DeliveryStockPlacement::Carried {
                holder: actor,
                same_place: false,
            })),
            SimTime(200),
        )
        .unwrap();
        assert_eq!(own.location, here);
        assert_eq!(own.observed_at, SimTime(200));
        assert_eq!(own.room, Some(Quantity(80)));
        assert!(!own.accessible);
    }
    #[test]
    fn remembered_stock_refresh_depends_on_actual_contact_and_preserves_dates_otherwise() {
        let memory = memory();
        let actor = PersonId(1);
        let here = Location::AtArea(AreaId(1));
        for (placement, visible) in [
            (DeliveryStockPlacement::Area(AreaId(1)), true),
            (DeliveryStockPlacement::Area(AreaId(3)), false),
            (
                DeliveryStockPlacement::Carried {
                    holder: PersonId(2),
                    same_place: true,
                },
                true,
            ),
            (
                DeliveryStockPlacement::Carried {
                    holder: PersonId(2),
                    same_place: false,
                },
                false,
            ),
        ] {
            let actual = Some(current(placement));
            let stock =
                delivery_stock_from_facts(&memory, actor, &here, actual.clone(), SimTime(200))
                    .unwrap();
            let destination = delivery_destination_from_facts(
                memory.inventory,
                Some(&memory),
                actor,
                &here,
                actual,
                SimTime(200),
            )
            .unwrap();
            assert_eq!(
                stock.quantity,
                if visible {
                    Quantity(20)
                } else {
                    memory.quantity
                }
            );
            assert_eq!(
                stock.observed_at,
                if visible {
                    SimTime(200)
                } else {
                    memory.observed_at
                }
            );
            assert_eq!(destination.observed_at, stock.observed_at);
            assert_eq!(destination.location, stock.location);
            assert_eq!(
                destination.room,
                if visible { Some(Quantity(80)) } else { None }
            );
            assert_eq!(destination.accessible, !visible);
        }
        let absent = delivery_stock_from_facts(
            &memory,
            actor,
            &Location::AtArea(memory.area),
            None,
            SimTime(200),
        )
        .unwrap();
        assert_eq!(absent.quantity, Quantity(0));
        assert_eq!(absent.observed_at, SimTime(200));
        assert!(
            delivery_destination_from_facts(
                memory.inventory,
                Some(&memory),
                actor,
                &here,
                None,
                SimTime(200)
            )
            .is_none()
        );
        assert!(
            delivery_stock_from_facts(
                &memory,
                actor,
                &here,
                Some(current(DeliveryStockPlacement::Area(AreaId(1)))),
                SimTime(109)
            )
            .is_none()
        );
    }
    #[test]
    fn acquisition_owner_classifies_consent_and_exact_personal_thresholds_in_input_order() {
        let offer = |action| game_logistics::AcquisitionOffer {
            key: action,
            good: Good::Food,
            source: None,
            location: Location::AtArea(AreaId(1)),
            observed_at: SimTime(0),
            accessible: true,
            quantity: game_logistics::OfferYield::Unknown,
            requires_carrying_room: false,
            requirements: vec![],
            action,
        };
        let kind = |action: &u64| match action {
            1 => AcquisitionKind::PropertyTaking,
            2 => AcquisitionKind::Gathering { unauthorized: true },
            3 => AcquisitionKind::Gathering {
                unauthorized: false,
            },
            _ => AcquisitionKind::Other,
        };
        for (honesty, risk_tolerance, accepts) in
            [(249, 700, true), (250, 700, false), (249, 699, false)]
        {
            let mut personality = Personality::industrious();
            personality.honesty = honesty;
            personality.risk_tolerance = risk_tolerance;
            let candidates = acquisition_candidates(
                &personality,
                [offer(2), offer(4), offer(1), offer(3)],
                kind,
            );
            assert_eq!(
                candidates
                    .iter()
                    .map(|candidate| (candidate.offer.action, candidate.willing))
                    .collect::<Vec<_>>(),
                vec![(2, accepts), (4, true), (1, accepts), (3, true)]
            );
        }
        assert_eq!(
            acquisition_request_quantity(Quantity(200), Quantity(50)),
            Quantity(50)
        );
        assert_eq!(
            acquisition_request_quantity(Quantity(20), Quantity(50)),
            Quantity(20)
        );
    }
}
