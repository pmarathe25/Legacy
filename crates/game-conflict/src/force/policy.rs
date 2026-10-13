use super::*;
use game_cargo::{Inventory, InventoryLocation};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersonFacts {
    pub alive: bool,
    pub health: u64,
    pub independent: bool,
    pub in_combat: bool,
    pub restricted: bool,
    pub idle_or_resting: bool,
    pub raw_location: Location,
    pub location: Location,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Willingness {
    pub aggression: u16,
    pub risk_tolerance: u16,
    pub trust: i32,
}
/// Physical and social facts only. Decisions and thresholds belong to Force.
pub trait Facts {
    fn person(&self, actor: PersonId) -> Option<PersonFacts>;
    /// Register the actual component rows supplying this person's body and
    /// location facts. This hook supplies reads and carries no decision policy.
    fn protect_person(&self, store: &Store, actor: PersonId, reads: &mut ReadSet) {
        reads.read::<Location>(store, StableId(actor.0));
    }
    fn protect_willingness(&self, _store: &Store, _actor: PersonId, _reads: &mut ReadSet) {}
    fn same_place(&self, actor: PersonId, target: PersonId) -> bool;
    fn combat_available(&self) -> bool;
    fn incapacitation(&self) -> u64;
    fn ruler(&self) -> PersonId;
    fn captor(&self, target: PersonId) -> Option<PersonId>;
    fn same_encounter(&self, target: PersonId, actor: PersonId) -> bool;
    fn carried(&self, actor: PersonId) -> Vec<InventoryId>;
    fn people(&self) -> impl Iterator<Item = PersonId>;
    fn willingness(&self, actor: PersonId, captor: PersonId) -> Option<Willingness>;
    /// Existing tactical positions in newest-encounter-first factual order.
    fn positions(&self, target: PersonId, area: AreaId) -> Vec<Point>;
}
pub fn area(person: &PersonFacts) -> Option<AreaId> {
    if !person.alive {
        return None;
    }
    match person.location {
        Location::AtArea(area) | Location::StoppedOnRoad { from: area, .. } => Some(area),
        Location::InTransit { from, .. }
            if matches!(person.raw_location, Location::Carried { .. }) =>
        {
            Some(from)
        }
        _ => None,
    }
}
pub fn actor_area(facts: &impl Facts, actor: PersonId) -> Option<AreaId> {
    area(&facts.person(actor)?)
}
pub fn body_ready(facts: &impl Facts, actor: PersonId) -> bool {
    facts.combat_available()
        && facts.person(actor).is_some_and(|person| {
            area(&person).is_some() && person.independent && person.health > facts.incapacitation()
        })
}
pub fn held_good(
    store: &Store,
    facts: &impl Facts,
    actor: PersonId,
    good: Good,
) -> Option<InventoryId> {
    facts.carried(actor).into_iter().find(|id| {
        store.get::<Inventory>(StableId(id.0)).is_some_and(|row| {
            row.location == InventoryLocation::Carried(actor)
                && row.good == good
                && row.quantity.0 >= 1000
        })
    })
}
pub fn surrendered(store: &Store, facts: &impl Facts, actor: PersonId, target: PersonId) -> bool {
    yielded(store, target).is_some_and(|yielded| {
        yielded.captors.contains(&actor) && Some(yielded.area) == actor_area(facts, target)
    })
}
pub fn restrainable(store: &Store, facts: &impl Facts, actor: PersonId, target: PersonId) -> bool {
    facts.combat_available()
        && facts.captor(target).is_none()
        && facts.person(target).is_some_and(|person| {
            area(&person).is_some()
                && (person.health <= facts.incapacitation()
                    || surrendered(store, facts, actor, target))
        })
}
fn track_person(plan: &mut PreparedChange, store: &Store, facts: &impl Facts, actor: PersonId) {
    facts.protect_person(store, actor, &mut plan.reads);
    plan.reads
        .read::<crate::Captivity>(store, StableId(actor.0));
}
fn track_equipment(plan: &mut PreparedChange, store: &Store, actor: PersonId, good: Good) {
    plan.reads.membership::<Inventory>(store, move |_, row| {
        row.location == InventoryLocation::Carried(actor)
            && row.good == good
            && row.quantity.0 >= 1000
    });
}
impl Runtime {
    pub fn prepare_yield(
        &self,
        store: &Store,
        facts: &impl Facts,
        actor: PersonId,
        captor: PersonId,
        now: SimTime,
    ) -> Result<PreparedChange, Error> {
        self.rules.validate()?;
        let person = facts.person(actor).ok_or(Error::Unavailable)?;
        if actor == captor
            || !person.independent
            || person.in_combat
            || area(&person).is_none()
            || !facts.same_place(actor, captor)
            || person.health == 0
            || !facts.combat_available()
        {
            return Err(Error::Unavailable);
        }
        let mut plan = PreparedChange::new();
        track_person(&mut plan, store, facts, actor);
        track_person(&mut plan, store, facts, captor);
        plan.yield_row(
            store,
            actor,
            Some(Yielded {
                captors: BTreeSet::from([captor]),
                area: area(&person).unwrap(),
                at: now,
            }),
        );
        plan.effects.push(Effect::Yielded { actor, captor });
        Ok(plan)
    }
    pub fn prepare_withdraw(
        &self,
        store: &Store,
        facts: &impl Facts,
        actor: PersonId,
    ) -> Result<PreparedChange, Error> {
        self.rules.validate()?;
        let person = facts.person(actor).ok_or(Error::Unavailable)?;
        if area(&person).is_none()
            || person.in_combat
            || !person.independent
            || yielded(store, actor).is_none()
        {
            return Err(Error::Unavailable);
        }
        let mut plan = PreparedChange::new();
        track_person(&mut plan, store, facts, actor);
        plan.yield_row(store, actor, None);
        plan.effects.push(Effect::YieldWithdrawn { actor });
        Ok(plan)
    }
    pub fn prepare_surrender(
        &self,
        store: &Store,
        actor: PersonId,
        area: AreaId,
        now: SimTime,
        team: u32,
        fighters: impl IntoIterator<Item = (PersonId, u32, bool)>,
    ) -> PreparedChange {
        let captors = fighters
            .into_iter()
            .filter(|(_, enemy, withdrawn)| *enemy != team && !*withdrawn)
            .map(|(actor, _, _)| actor)
            .collect::<BTreeSet<_>>();
        let mut plan = PreparedChange::new();
        plan.yield_row(
            store,
            actor,
            (!captors.is_empty()).then_some(Yielded {
                captors,
                area,
                at: now,
            }),
        );
        plan
    }
    pub fn prepare_restraint(
        &self,
        store: &Store,
        facts: &impl Facts,
        actor: PersonId,
        target: PersonId,
        now: SimTime,
    ) -> Result<PreparedChange, Error> {
        self.rules.validate()?;
        let person = facts.person(actor).ok_or(Error::Unavailable)?;
        let victim = facts.person(target).ok_or(Error::Unavailable)?;
        if actor == target
            || !body_ready(facts, actor)
            || person.restricted
            || victim.in_combat
            || !facts.same_place(actor, target)
            || !restrainable(store, facts, actor, target)
        {
            return Err(Error::Unavailable);
        }
        let binding = held_good(store, facts, actor, Good::Binding).ok_or(Error::Unavailable)?;
        let area = area(&victim).unwrap();
        let row = Restraint {
            target,
            binding,
            area,
            location: person.location,
            position: facts
                .positions(target, area)
                .into_iter()
                .next()
                .unwrap_or(Point { x: 6000, y: 8000 }),
            completes: SimTime(
                now.0
                    .checked_add(self.rules.restraint_time.0)
                    .ok_or(Error::Overflow)?,
            ),
        };
        let mut plan = PreparedChange::new();
        track_person(&mut plan, store, facts, actor);
        track_person(&mut plan, store, facts, target);
        track_equipment(&mut plan, store, actor, Good::Binding);
        plan.reads.read::<Inventory>(store, StableId(binding.0));
        plan.reads.read::<Yielded>(store, StableId(target.0));
        plan.restraint_row(store, actor, Some(row.clone()));
        plan.work = Some(Work::Restrain { actor, row });
        Ok(plan)
    }
    pub fn prepare_escape(
        &self,
        store: &Store,
        facts: &impl Facts,
        actor: PersonId,
        now: SimTime,
    ) -> Result<PreparedChange, Error> {
        self.rules.validate()?;
        if !body_ready(facts, actor)
            || facts.person(actor).is_none_or(|person| person.in_combat)
            || facts.captor(actor).is_none()
        {
            return Err(Error::Unavailable);
        }
        let tool = held_good(store, facts, actor, Good::Tool).ok_or(Error::Unavailable)?;
        let completes = SimTime(
            now.0
                .checked_add(self.rules.escape_time.0)
                .ok_or(Error::Overflow)?,
        );
        let mut plan = self.prepare_release_started(store, facts, actor, actor, now, completes)?;
        plan.work = Some(Work::Escape {
            actor,
            row: Escape {
                target: actor,
                actor,
                tool: Some(tool),
                began: now,
                completes,
                contested: false,
            },
        });
        plan.effects.push(Effect::EscapeAttempted { actor });
        Ok(plan)
    }
    pub fn prepare_release_started(
        &self,
        store: &Store,
        facts: &impl Facts,
        actor: PersonId,
        target: PersonId,
        now: SimTime,
        completes: SimTime,
    ) -> Result<PreparedChange, Error> {
        self.rules.validate()?;
        if completes <= now || facts.captor(target).is_none() {
            return Err(Error::Unavailable);
        }
        let tool = held_good(store, facts, actor, Good::Tool);
        let mut plan = PreparedChange::new();
        track_person(&mut plan, store, facts, actor);
        track_person(&mut plan, store, facts, target);
        track_equipment(&mut plan, store, actor, Good::Tool);
        if let Some(tool) = tool {
            plan.reads.read::<Inventory>(store, StableId(tool.0));
        }
        plan.escape_row(
            store,
            target,
            Some(Escape {
                target,
                actor,
                tool,
                began: now,
                completes,
                contested: false,
            }),
        );
        Ok(plan)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Intervention {
    pub target: PersonId,
    pub participants: Vec<(PersonId, u32)>,
}
impl Runtime {
    fn select_intervention(&self, facts: &impl Facts, attempt: &Escape) -> Option<Intervention> {
        let target = attempt.target;
        let captor = facts.captor(target)?;
        if attempt.actor == captor {
            return None;
        }
        let candidates = facts.people().collect::<BTreeSet<_>>();
        let opponents = candidates
            .into_iter()
            .filter(|person| {
                *person != facts.ruler()
                    && *person != attempt.actor
                    && *person != target
                    && facts.same_place(*person, target)
                    && facts.person(*person).is_some_and(|body| {
                        body.alive
                            && body.independent
                            && body.health > facts.incapacitation()
                            && !body.restricted
                    })
            })
            .filter(|person| {
                facts.willingness(*person, captor).is_some_and(|mind| {
                    if *person == captor {
                        mind.risk_tolerance >= 500 && mind.aggression >= 500
                    } else {
                        mind.risk_tolerance >= 700 && mind.aggression >= 700 && mind.trust >= 500
                    }
                })
            })
            .take(8)
            .collect::<Vec<_>>();
        if opponents.is_empty() {
            return None;
        }
        let mut participants = vec![(attempt.actor, 0)];
        if attempt.actor != target {
            participants.push((target, 0));
        }
        participants.extend(opponents.into_iter().map(|actor| (actor, 1)));
        Some(Intervention {
            target,
            participants,
        })
    }
    fn prepare_started_work(
        &self,
        store: &Store,
        facts: &impl Facts,
        actor: PersonId,
        work: PreparedChange,
    ) -> (PreparedChange, Option<Intervention>) {
        let mut plan = prepare_interrupt_many(store, &[actor]);
        plan.replace_with(work);
        let attempt = plan.changes.iter().find_map(|change| {
            if let Change::Escape(_, Some(row)) = change {
                Some(row.clone())
            } else {
                None
            }
        });
        let intervention = attempt
            .as_ref()
            .and_then(|attempt| self.select_intervention(facts, attempt));
        if let Some(intervention) = &intervention {
            for (person, _) in &intervention.participants {
                track_person(&mut plan, store, facts, *person);
                facts.protect_willingness(store, *person, &mut plan.reads);
            }
            for change in &mut plan.changes {
                if let Change::Escape(target, Some(row)) = change
                    && *target == intervention.target
                {
                    row.contested = true;
                }
            }
            if let Some(Work::Escape { row, .. }) = &mut plan.work {
                row.contested = true;
            }
        }
        (plan, intervention)
    }
    pub fn prepare_restraint_start(
        &self,
        store: &Store,
        facts: &impl Facts,
        actor: PersonId,
        target: PersonId,
        now: SimTime,
    ) -> Result<PreparedChange, Error> {
        let work = self.prepare_restraint(store, facts, actor, target, now)?;
        let mut plan = prepare_interrupt_many(store, &[actor]);
        plan.replace_with(work);
        Ok(plan)
    }
    pub fn prepare_escape_start(
        &self,
        store: &Store,
        facts: &impl Facts,
        actor: PersonId,
        now: SimTime,
    ) -> Result<(PreparedChange, Option<Intervention>), Error> {
        let work = self.prepare_escape(store, facts, actor, now)?;
        Ok(self.prepare_started_work(store, facts, actor, work))
    }
    pub fn prepare_release_start(
        &self,
        store: &Store,
        facts: &impl Facts,
        actor: PersonId,
        target: PersonId,
        now: SimTime,
        completes: SimTime,
    ) -> Result<(PreparedChange, Option<Intervention>), Error> {
        let work = self.prepare_release_started(store, facts, actor, target, now, completes)?;
        Ok(self.prepare_started_work(store, facts, actor, work))
    }
    pub fn prepare_intervention(
        &self,
        store: &Store,
        facts: &impl Facts,
        target: PersonId,
    ) -> Result<Option<(PreparedChange, Intervention)>, Error> {
        self.rules.validate()?;
        let attempt = escape(store, target).ok_or(Error::Unavailable)?;
        facts.captor(target).ok_or(Error::Unavailable)?;
        let Some(intervention) = self.select_intervention(facts, attempt) else {
            return Ok(None);
        };
        let mut plan = PreparedChange::new();
        let mut after = attempt.clone();
        after.contested = true;
        for (actor, _) in &intervention.participants {
            track_person(&mut plan, store, facts, *actor);
            facts.protect_willingness(store, *actor, &mut plan.reads);
        }
        plan.escape_row(store, target, Some(after));
        Ok(Some((plan, intervention)))
    }
    pub fn completion(
        &self,
        store: &Store,
        facts: &impl Facts,
        actor: PersonId,
        now: SimTime,
    ) -> Option<Completion> {
        if let Some(row) = restraint(store, actor).filter(|row| row.completes <= now) {
            let row = row.clone();
            let mut plan = PreparedChange::new();
            plan.restraint_row(store, actor, None);
            return Some(Completion::Restrain { row, removal: plan });
        }
        let attempt = escape(store, actor)
            .filter(|row| row.actor == actor && !row.contested && row.completes <= now)?;
        let successful = body_ready(facts, actor)
            && facts.captor(actor).is_some()
            && attempt.tool.is_some_and(|tool| {
                store.get::<Inventory>(StableId(tool.0)).is_some_and(|row| {
                    row.good == Good::Tool
                        && row.location == InventoryLocation::Carried(actor)
                        && row.quantity.0 >= 1000
                })
            });
        let mut plan = PreparedChange::new();
        track_person(&mut plan, store, facts, actor);
        if let Some(tool) = attempt.tool {
            plan.reads.read::<Inventory>(store, StableId(tool.0));
        }
        plan.escape_row(store, actor, None);
        if !successful {
            plan.effects.push(Effect::EscapeInterrupted { actor });
        }
        Some(Completion::Escape {
            successful,
            removal: plan,
        })
    }
    pub fn prepare_update(
        &self,
        store: &Store,
        facts: &impl Facts,
        invalidated: impl IntoIterator<Item = ObservedEvent>,
    ) -> PreparedChange {
        let mut plan = PreparedChange::new();
        plan.reads.membership::<Escape>(store, |_, _| true);
        for (target, attempt) in escapes(store) {
            plan.reads.read::<Escape>(store, StableId(target.0));
            plan.reads
                .read::<crate::Captivity>(store, StableId(target.0));
            if facts.captor(target).is_none()
                || attempt.contested && !facts.same_encounter(target, attempt.actor)
            {
                plan.escape_row(store, target, None);
            }
        }
        let invalidated = invalidated
            .into_iter()
            .filter_map(|event| match event {
                ObservedEvent::Departed { actor } | ObservedEvent::Struck { actor } => Some(actor),
                ObservedEvent::Other => None,
            })
            .collect::<BTreeSet<_>>();
        plan.reads.membership::<Yielded>(store, |_, _| true);
        for (actor, row) in yields(store) {
            plan.reads.read::<Yielded>(store, StableId(actor.0));
            let mut after = row.clone();
            after.captors.retain(|actor| !invalidated.contains(actor));
            if invalidated.contains(&actor) || after.captors.is_empty() {
                plan.yield_row(store, actor, None);
            } else if &after != row {
                plan.yield_row(store, actor, Some(after));
            }
        }
        plan
    }
    pub fn allocation_bound(
        &self,
        store: &Store,
        actors: impl IntoIterator<Item = PersonId>,
    ) -> u64 {
        actors
            .into_iter()
            .filter(|actor| restraint(store, *actor).is_some())
            .count() as u64
            * 2
    }
}
#[derive(Debug)]
pub enum Completion {
    Escape {
        successful: bool,
        removal: PreparedChange,
    },
    Restrain {
        row: Restraint,
        removal: PreparedChange,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Choice {
    Escape,
    WithdrawYield,
    Yield {
        captor: PersonId,
    },
    Restrain {
        target: PersonId,
    },
    FreeCaptive {
        target: PersonId,
    },
    CarryTool {
        source: InventoryId,
        quantity: Quantity,
    },
}
pub fn escape_candidate(store: &Store, facts: &impl Facts, actor: PersonId) -> Option<Choice> {
    (body_ready(facts, actor)
        && facts
            .person(actor)
            .is_some_and(|person| person.idle_or_resting && !person.in_combat)
        && facts.captor(actor).is_some()
        && held_good(store, facts, actor, Good::Tool).is_some())
    .then_some(Choice::Escape)
}
pub fn guardian_rescue(
    store: &Store,
    facts: &impl Facts,
    guardian: PersonId,
    target: PersonId,
    known: impl IntoIterator<Item = StockKnowledge>,
) -> Option<Choice> {
    if facts.captor(target).is_none()
        || !facts.same_place(guardian, target)
        || !body_ready(facts, guardian)
        || facts.person(guardian)?.restricted
    {
        return None;
    }
    if held_good(store, facts, guardian, Good::Tool).is_some() {
        return Some(Choice::FreeCaptive { target });
    }
    let person = facts.person(guardian)?;
    let tool = known.into_iter().find(|fact| {
        fact.good == Good::Tool
            && Some(fact.area) == area(&person)
            && matches!(person.location, Location::AtArea(_))
            && fact.accessible
            && fact.quantity.0 >= 1000
    })?;
    Some(Choice::CarryTool {
        source: tool.inventory,
        quantity: Quantity(1000),
    })
}
pub fn available_choices(store: &Store, facts: &impl Facts, actor: PersonId) -> Vec<Choice> {
    let Some(person) = facts.person(actor) else {
        return vec![];
    };
    if !facts.combat_available()
        || area(&person).is_none()
        || !person.independent
        || person.in_combat
    {
        return vec![];
    }
    let mut choices = vec![];
    if let Some(choice) = escape_candidate(store, facts, actor) {
        choices.push(choice);
    }
    if yielded(store, actor).is_some() {
        choices.push(Choice::WithdrawYield);
    }
    for target in facts
        .people()
        .filter(|target| *target != actor && facts.same_place(actor, *target))
        .take(8)
    {
        choices.push(Choice::Yield { captor: target });
        if body_ready(facts, actor)
            && !person.restricted
            && facts.person(target).is_some_and(|person| !person.in_combat)
            && held_good(store, facts, actor, Good::Binding).is_some()
            && restrainable(store, facts, actor, target)
        {
            choices.push(Choice::Restrain { target });
        }
    }
    choices
}
pub fn passive_participant(facts: &impl Facts, actor: PersonId) -> bool {
    actor_area(facts, actor).is_some() && facts.captor(actor).is_some()
}
pub fn can_combat_act(store: &Store, facts: &impl Facts, actor: PersonId) -> bool {
    body_ready(facts, actor) && escape(store, actor).is_some_and(|escape| escape.contested)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CombatChoice {
    Defend,
    Wait,
    Release { target: PersonId },
    Other,
}
pub fn action_allowed(
    store: &Store,
    facts: &impl Facts,
    actor: PersonId,
    movement: bool,
    action: CombatChoice,
) -> bool {
    facts.captor(actor).is_none()
        || !movement
            && match action {
                CombatChoice::Defend | CombatChoice::Wait => true,
                CombatChoice::Release { target } => {
                    target == actor && held_good(store, facts, actor, Good::Tool).is_some()
                }
                CombatChoice::Other => false,
            }
}
pub fn rescue_target(store: &Store, facts: &impl Facts, actor: PersonId) -> Option<PersonId> {
    escapes(store)
        .map(|(_, row)| row)
        .find(|row| row.actor == actor && row.target != actor && facts.captor(row.target).is_some())
        .map(|row| row.target)
}
pub fn combat_action(
    store: &Store,
    facts: &impl Facts,
    actor: PersonId,
    rescue_reachable: bool,
) -> Option<CombatChoice> {
    if passive_participant(facts, actor) {
        return Some(if held_good(store, facts, actor, Good::Tool).is_some() {
            CombatChoice::Release { target: actor }
        } else {
            CombatChoice::Defend
        });
    }
    let target = rescue_target(store, facts, actor)?;
    (rescue_reachable && held_good(store, facts, actor, Good::Tool).is_some())
        .then_some(CombatChoice::Release { target })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfinementView {
    pub target: PersonId,
    pub captor: PersonId,
    pub location: Location,
    pub captured_position: Point,
    pub can_escape: bool,
    pub escaping: bool,
}
pub fn confinement_views<'a>(
    store: &Store,
    facts: &impl Facts,
    actor: PersonId,
    physical: bool,
    custody: impl IntoIterator<Item = &'a crate::Captivity>,
) -> Vec<ConfinementView> {
    if !physical || facts.person(actor).is_none_or(|person| !person.alive) {
        return vec![];
    }
    custody
        .into_iter()
        .filter(|captive| facts.same_place(actor, captive.actor))
        .take(32)
        .filter_map(|captive| {
            let location = facts.person(captive.actor)?.location;
            Some(ConfinementView {
                target: captive.actor,
                captor: captive.captor,
                location,
                captured_position: captive.position,
                can_escape: captive.actor == actor
                    && body_ready(facts, actor)
                    && held_good(store, facts, actor, Good::Tool).is_some(),
                escaping: escape(store, captive.actor).is_some(),
            })
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObservedEvent {
    Departed { actor: PersonId },
    Struck { actor: PersonId },
    Other,
}
