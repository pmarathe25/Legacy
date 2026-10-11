use crate::*;
use game_core::*;
use game_ecs::{Batch, Store};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Active {
    pub actor: PersonId,
    pub lives: BTreeSet<PersonId>,
    pub started: SimTime,
    pub accepted: BTreeMap<InvocationCost, Consent>,
    pub episode: Option<Episode>,
    pub next_loss_burden: u64,
    pub next_controller: SimTime,
    pub dominant: Option<PersonId>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Activation {
    pub actor: PersonId,
    pub lives: BTreeSet<PersonId>,
    pub accepted: BTreeMap<InvocationCost, Consent>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvocationWarning {
    pub id: WarningId,
    pub actor: PersonId,
    pub lives: BTreeSet<PersonId>,
    pub cost: InvocationCost,
    pub severity: u8,
    pub activation_burden: u64,
    pub instant_fatigue: u64,
    pub fatigue_per_mille: u64,
    pub health_harm_per_mille: u64,
    pub identity_skill_penalty: u16,
    pub emotion_disruption: u16,
    pub lifespan_loss: SimDuration,
    pub control_loss_duration: SimDuration,
    pub permanent: bool,
    pub description: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Warning {
    pub id: WarningId,
    pub activation: Option<BTreeSet<PersonId>>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlState {
    pub rules: Rules,
    pub active: Option<Active>,
    pub activation: Option<Activation>,
    pub warning: Option<Warning>,
}
impl ControlState {
    pub fn new(rules: Rules) -> Result<Self, Error> {
        validate_rules(&rules)?;
        Ok(Self {
            rules,
            active: None,
            activation: None,
            warning: None,
        })
    }
}
/// Current-life/body and actual guidance come from their owners. The archive
/// supplies only eligible dead-life IDs here; no live NPC knowledge is queried.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdmissionFacts {
    pub actor: PersonId,
    pub now: SimTime,
    pub current: Option<PersonId>,
    pub alive: bool,
    pub health: u64,
    pub independent: bool,
    pub learning: u64,
    pub required_learning: u64,
    pub fatigue: u64,
    pub fatigue_limit: u64,
    pub archived: BTreeSet<PersonId>,
}
#[derive(Clone, Debug)]
pub enum ControlAction {
    Activate { lives: BTreeSet<PersonId> },
    Dismiss,
    AcceptWarning { warning: WarningId },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ControlEffect {
    WarningRaised {
        warning: WarningId,
        actor: PersonId,
    },
    WarningAccepted {
        warning: WarningId,
        actor: PersonId,
    },
    Started {
        actor: PersonId,
        lives: BTreeSet<PersonId>,
    },
    Dismissed {
        actor: PersonId,
    },
    FatigueCharge {
        actor: PersonId,
        quantity: u64,
    },
    CostApplied {
        actor: PersonId,
        cost: InvocationCost,
        severity: u8,
        permanent: bool,
    },
    ControlChanged {
        actor: PersonId,
        controlled: bool,
        until: Option<SimTime>,
    },
    LifespanChanged {
        actor: PersonId,
    },
    DeathCleanup {
        actor: PersonId,
    },
    ControllerSelected {
        actor: PersonId,
        life: PersonId,
        changed: bool,
    },
}
/// A prepared frame copies only the one control episode and touched burdens,
/// never archives, physical stock, recorded memories, or an entire simulation.
#[derive(Clone, Debug)]
pub struct ControlFinalization {
    active: Option<Active>,
    activation: Option<Activation>,
    warning: Option<Warning>,
}
impl ControlFinalization {
    /// Publish metadata only after the shared component batch has succeeded.
    pub fn finish(self, state: &mut ControlState) {
        state.active = self.active;
        state.activation = self.activation;
        state.warning = self.warning;
    }
}
#[derive(Clone, Debug)]
pub struct PreparedControl {
    at: SimTime,
    admission: Option<AdmissionFacts>,
    rules: Rules,
    before_active: Option<Active>,
    before_activation: Option<Activation>,
    before_warning: Option<Warning>,
    before_warning_record: Option<InvocationWarning>,
    active: Option<Active>,
    activation: Option<Activation>,
    warning: Option<Warning>,
    burdens: BTreeMap<PersonId, (Option<Burden>, Burden)>,
    new_warning: Option<InvocationWarning>,
    pub effects: Vec<ControlEffect>,
}
impl PreparedControl {
    fn frame(state: &ControlState, store: &Store, at: SimTime) -> Self {
        Self {
            at,
            admission: None,
            rules: state.rules.clone(),
            before_active: state.active.clone(),
            before_activation: state.activation.clone(),
            before_warning: state.warning.clone(),
            before_warning_record: state.pending_warning(store).cloned(),
            active: state.active.clone(),
            activation: state.activation.clone(),
            warning: state.warning.clone(),
            burdens: BTreeMap::new(),
            new_warning: None,
            effects: Vec::new(),
        }
    }
    pub fn check(&self, state: &ControlState, store: &Store) -> Result<(), Error> {
        if state.rules != self.rules
            || state.active != self.before_active
            || state.activation != self.before_activation
            || state.warning != self.before_warning
            || state.pending_warning(store) != self.before_warning_record.as_ref()
            || self
                .burdens
                .iter()
                .any(|(p, (old, _))| burden(store, *p) != old.as_ref())
            || self
                .new_warning
                .as_ref()
                .is_some_and(|w| warning(store, w.id).is_some())
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub fn stage_into(
        self,
        state: &ControlState,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<ControlFinalization, Error> {
        self.check(state, store)?;
        for (actor, (_, value)) in self.burdens {
            batch.insert(game_ecs::StableId(actor.0), value);
        }
        if let Some(value) = self.new_warning {
            batch.insert(game_ecs::StableId(value.id.0), value);
        }
        Ok(ControlFinalization {
            active: self.active,
            activation: self.activation,
            warning: self.warning,
        })
    }
    pub fn commit(self, state: &mut ControlState, store: &mut Store) -> Result<(), Error> {
        let mut batch = Batch::new();
        let finalization = self.stage_into(state, store, &mut batch)?;
        store.apply_batch(batch)?;
        finalization.finish(state);
        Ok(())
    }
    pub fn at(&self) -> SimTime {
        self.at
    }
    pub fn admission(&self) -> Option<&AdmissionFacts> {
        self.admission.as_ref()
    }
    pub fn warning_id(&self) -> Option<WarningId> {
        self.new_warning.as_ref().map(|w| w.id)
    }
    fn burden<'a>(&'a mut self, store: &Store, actor: PersonId) -> &'a mut Burden {
        &mut self
            .burdens
            .entry(actor)
            .or_insert_with(|| {
                let old = burden(store, actor).cloned();
                (old.clone(), old.unwrap_or_default())
            })
            .1
    }
    fn refresh(&mut self, store: &Store, now: SimTime, alive: bool) {
        if let Some(a) = self.active.clone() {
            let controlled = controls_actor(a.episode.as_ref(), a.actor, now, alive);
            let before = self.burden(store, a.actor).clone();
            *self.burden(store, a.actor) =
                refresh_temporary(&self.rules, &before, &a.accepted, controlled);
        }
    }
    fn permanent(
        &mut self,
        store: &Store,
        actor: PersonId,
        req: CostRequirement,
    ) -> Result<(), Error> {
        let before = self.burden(store, actor).clone();
        let change = apply_permanent(&self.rules, &before, req)?;
        *self.burden(store, actor) = change.burden;
        if change.changed {
            self.effects.push(ControlEffect::CostApplied {
                actor,
                cost: req.cost,
                severity: req.severity,
                permanent: true,
            });
            if req.cost == InvocationCost::Lifespan {
                self.effects.push(ControlEffect::LifespanChanged { actor });
            }
        }
        Ok(())
    }
    fn episode(&mut self, store: &Store, actor: PersonId, now: SimTime) -> Result<(), Error> {
        let a = self
            .active
            .as_ref()
            .filter(|a| a.actor == actor)
            .ok_or(Error::Unavailable)?;
        let consent = a
            .accepted
            .get(&InvocationCost::LossOfControl)
            .cloned()
            .unwrap_or_default();
        let before = self.burden(store, actor).clone();
        let start = begin_episode(&self.rules, &before, actor, now, &consent)?;
        let until = start.episode.ends;
        let a = self.active.as_mut().unwrap();
        a.next_loss_burden = start.next_loss_burden;
        a.next_controller = start.next_controller;
        a.episode = Some(start.episode);
        self.effects.push(ControlEffect::ControlChanged {
            actor,
            controlled: false,
            until: Some(until),
        });
        Ok(())
    }
    fn charge(
        &mut self,
        store: &Store,
        actor: PersonId,
        lives: usize,
        f: &AdmissionFacts,
    ) -> Result<ActivationCharge, Error> {
        let before = self.burden(store, actor).clone();
        let charge = activation_charge(&self.rules, &before, lives)?;
        if f.fatigue
            .checked_add(charge.fatigue)
            .is_none_or(|n| n > f.fatigue_limit)
        {
            return Err(Error::Unavailable);
        }
        Ok(charge)
    }
    fn review(
        &mut self,
        store: &Store,
        f: &AdmissionFacts,
        id: Option<WarningId>,
    ) -> Result<(), Error> {
        if self.warning.is_some() {
            return Ok(());
        }
        let planned = self.activation.clone();
        let (actor, lives, accepted, next_loss) = if let Some(p) = &planned {
            (
                p.actor,
                p.lives.clone(),
                p.accepted.clone(),
                self.rules.loss_threshold,
            )
        } else {
            let Some(a) = &self.active else {
                return Ok(());
            };
            if controls_actor(a.episode.as_ref(), a.actor, f.now, f.alive) {
                return Ok(());
            }
            (
                a.actor,
                a.lives.clone(),
                a.accepted.clone(),
                a.next_loss_burden,
            )
        };
        let mut projected = self.burden(store, actor).clone();
        let charge = if planned.is_some() {
            Some(self.charge(store, actor, lives.len(), f)?)
        } else {
            None
        };
        if let Some(charge) = charge {
            projected.burden = projected
                .burden
                .checked_add(charge.burden)
                .ok_or(Error::Overflow)?;
        }
        if let Some(req) = next_warning(
            &self.rules,
            lives.len(),
            &projected,
            next_loss,
            &accepted,
            planned.is_some(),
            false,
        ) {
            let id = id
                .filter(|id| id.0 != 0 && warning(store, *id).is_none())
                .ok_or(Error::Unavailable)?;
            let mut view = warning_spec(&self.rules, actor, &lives, req)?;
            view.id = id;
            if planned.is_some() && !accepted.contains_key(&InvocationCost::Strain) {
                let charge = charge.unwrap();
                view.activation_burden = charge.burden;
                view.instant_fatigue = charge.fatigue;
                view.description="Activation pays the shown fatigue and adds persistent burden before any borrowed benefit begins. Full payment is required; dismissal does not refund it.".into();
            }
            self.warning = Some(Warning {
                id,
                activation: planned.map(|p| p.lives),
            });
            self.new_warning = Some(view);
            self.effects
                .push(ControlEffect::WarningRaised { warning: id, actor });
        } else if let Some(p) = planned {
            let charge = charge.unwrap();
            self.burden(store, actor).burden = projected.burden;
            self.activation = None;
            self.active = Some(Active {
                actor,
                lives: p.lives.clone(),
                started: f.now,
                accepted: p.accepted.clone(),
                episode: None,
                next_loss_burden: self.rules.loss_threshold,
                next_controller: f.now,
                dominant: None,
            });
            self.effects.push(ControlEffect::FatigueCharge {
                actor,
                quantity: charge.fatigue,
            });
            self.effects.push(ControlEffect::Started {
                actor,
                lives: p.lives,
            });
            self.effects.push(ControlEffect::CostApplied {
                actor,
                cost: InvocationCost::Strain,
                severity: 0,
                permanent: false,
            });
            self.refresh(store, f.now, f.alive);
            for (cost, consent) in &p.accepted {
                if consent.permanent > 0 {
                    self.permanent(
                        store,
                        actor,
                        CostRequirement {
                            cost: *cost,
                            severity: consent.permanent,
                            permanent: true,
                        },
                    )?;
                }
            }
            if p.accepted
                .get(&InvocationCost::LossOfControl)
                .is_some_and(|c| c.temporary > 0)
            {
                self.episode(store, actor, f.now)?;
            }
        }
        Ok(())
    }
}
impl ControlState {
    pub fn pending_warning<'a>(&self, store: &'a Store) -> Option<&'a InvocationWarning> {
        warning(store, self.warning.as_ref()?.id)
    }
    pub fn ready(&self, actor: PersonId, f: &AdmissionFacts) -> bool {
        f.current == Some(actor) && f.alive && f.independent && f.learning >= f.required_learning
    }
    pub fn prepare_action(
        &self,
        store: &Store,
        actor: PersonId,
        action: &ControlAction,
        f: &AdmissionFacts,
        id: Option<WarningId>,
    ) -> Result<PreparedControl, Error> {
        if f.actor != actor
            || self.active.as_ref().is_some_and(|a| a.actor != actor)
            || !f.alive
            || f.health == 0
            || controls_actor(
                self.active.as_ref().and_then(|a| a.episode.as_ref()),
                actor,
                f.now,
                f.alive,
            )
        {
            return Err(Error::Unavailable);
        }
        let mut plan = PreparedControl::frame(self, store, f.now);
        plan.admission = Some(f.clone());
        match action {
            ControlAction::Activate { lives } => {
                if !self.ready(actor, f)
                    || lives.is_empty()
                    || lives.len() > 8
                    || !lives.is_subset(&f.archived)
                    || lives.contains(&actor)
                    || self.warning.is_some()
                    || self.active.as_ref().is_some_and(|a| a.lives == *lives)
                {
                    return Err(Error::Unavailable);
                }
                plan.charge(store, actor, lives.len(), f)?;
                plan.activation = Some(Activation {
                    actor,
                    lives: lives.clone(),
                    accepted: BTreeMap::new(),
                });
                plan.review(store, f, id)?;
            }
            ControlAction::Dismiss => {
                if !self.active.as_ref().is_some_and(|a| a.actor == actor)
                    && !self.warning.as_ref().is_some_and(|w| {
                        warning(store, w.id).is_some_and(|view| view.actor == actor)
                    })
                {
                    return Err(Error::Unavailable);
                }
                plan.active = None;
                plan.activation = None;
                plan.warning = None;
                if burden(store, actor).is_some() {
                    plan.burden(store, actor).uninterrupted = 0;
                }
                plan.effects.push(ControlEffect::Dismissed { actor });
            }
            ControlAction::AcceptWarning { warning } => {
                let pending = self
                    .warning
                    .as_ref()
                    .filter(|p| {
                        p.id == *warning
                            && crate::warning(store, p.id).is_some_and(|view| view.actor == actor)
                    })
                    .ok_or(Error::Unavailable)?;
                let view = crate::warning(store, pending.id).ok_or(Error::Unavailable)?;
                let req = CostRequirement {
                    cost: view.cost,
                    severity: view.severity,
                    permanent: view.permanent,
                };
                plan.warning = None;
                if pending.activation.is_some() {
                    let p = plan
                        .activation
                        .as_mut()
                        .filter(|p| {
                            p.actor == actor && Some(&p.lives) == pending.activation.as_ref()
                        })
                        .ok_or(Error::Unavailable)?;
                    p.accepted = accept_requirement(&p.accepted, req);
                    plan.effects.push(ControlEffect::WarningAccepted {
                        warning: *warning,
                        actor,
                    });
                    plan.review(store, f, id)?;
                } else {
                    let a = plan
                        .active
                        .as_mut()
                        .filter(|a| a.actor == actor)
                        .ok_or(Error::Unavailable)?;
                    a.accepted = accept_requirement(&a.accepted, req);
                    plan.refresh(store, f.now, f.alive);
                    plan.effects.push(ControlEffect::WarningAccepted {
                        warning: *warning,
                        actor,
                    });
                    if req.cost == InvocationCost::LossOfControl {
                        plan.episode(store, actor, f.now)?;
                    } else {
                        if req.permanent {
                            plan.permanent(store, actor, req)?;
                        }
                        plan.review(store, f, id)?;
                    }
                }
            }
        }
        Ok(plan)
    }
    /// Reconciles the current body's actual survival and consent. An accepted
    /// control episode authorizes all ensuing costs, without dismissal/capping.
    pub fn prepare_review(
        &self,
        store: &Store,
        f: &AdmissionFacts,
        id: Option<WarningId>,
    ) -> Result<PreparedControl, Error> {
        let owner = self
            .active
            .as_ref()
            .map(|a| a.actor)
            .or_else(|| self.pending_warning(store).map(|view| view.actor))
            .or_else(|| self.activation.as_ref().map(|a| a.actor));
        if owner.is_some_and(|actor| actor != f.actor) {
            return Err(Error::Unavailable);
        }
        let mut plan = PreparedControl::frame(self, store, f.now);
        plan.admission = Some(f.clone());
        if let Some(a) = &self.active {
            if !f.alive {
                plan.active = None;
                plan.activation = None;
                plan.warning = None;
                plan.burden(store, a.actor).uninterrupted = 0;
                plan.effects
                    .push(ControlEffect::DeathCleanup { actor: a.actor });
                return Ok(plan);
            }
            plan.refresh(store, f.now, true);
            if controls_actor(a.episode.as_ref(), a.actor, f.now, true) {
                let before = plan.burden(store, a.actor).clone();
                for req in required_bands(&self.rules, a.lives.len(), &before, a.next_loss_burden)
                    .into_iter()
                    .filter(|r| r.permanent)
                {
                    plan.permanent(store, a.actor, req)?;
                }
            }
            if a.episode.as_ref().is_some_and(|e| f.now >= e.ends) {
                let a = plan.active.as_mut().unwrap();
                a.episode = None;
                a.dominant = None;
                plan.effects.push(ControlEffect::ControlChanged {
                    actor: a.actor,
                    controlled: true,
                    until: None,
                });
            }
        } else if let Some(w) = &self.warning
            && !f.alive
        {
            plan.warning = None;
            plan.activation = None;
            plan.effects.push(ControlEffect::DeathCleanup {
                actor: warning(store, w.id).ok_or(Error::Unavailable)?.actor,
            });
            return Ok(plan);
        }
        plan.review(store, f, id)?;
        Ok(plan)
    }
    /// Actual body death is a physical-owner fact, never a player dismissal.
    /// Clears only that person's episode and continuity; permanent costs remain
    /// bound to their archived person and are never moved to a successor.
    pub fn prepare_deaths(
        &self,
        store: &Store,
        dead: &BTreeSet<PersonId>,
        now: SimTime,
    ) -> PreparedControl {
        let mut plan = PreparedControl::frame(self, store, now);
        for actor in dead {
            if burden(store, *actor).is_some_and(|b| b.uninterrupted > 0) {
                plan.burden(store, *actor).uninterrupted = 0;
            }
        }
        let owner = self
            .active
            .as_ref()
            .map(|a| a.actor)
            .or_else(|| self.pending_warning(store).map(|view| view.actor))
            .or_else(|| self.activation.as_ref().map(|a| a.actor));
        if let Some(actor) = owner
            && dead.contains(&actor)
        {
            plan.active = None;
            plan.activation = None;
            plan.warning = None;
            plan.effects.push(ControlEffect::DeathCleanup { actor });
        }
        plan
    }
    /// Only selects the due historical controller. Its private memories/goals
    /// and actual physical commands remain with their respective owners.
    pub fn prepare_controller(
        &self,
        store: &Store,
        now: SimTime,
        alive: bool,
    ) -> Result<Option<PreparedControl>, Error> {
        let Some(a) = &self.active else {
            return Ok(None);
        };
        if !controls_actor(a.episode.as_ref(), a.actor, now, alive) || a.next_controller > now {
            return Ok(None);
        }
        let episode = a.episode.as_ref().unwrap();
        let epoch = (now.0 - episode.starts.0) / self.rules.controller_quantum.0;
        let life = *a
            .lives
            .iter()
            .nth(epoch as usize % a.lives.len())
            .ok_or(Error::Unavailable)?;
        let next = SimTime(
            now.0
                .checked_add(self.rules.controller_quantum.0)
                .ok_or(Error::Overflow)?
                .min(episode.ends.0),
        );
        let mut plan = PreparedControl::frame(self, store, now);
        let active = plan.active.as_mut().unwrap();
        active.dominant = Some(life);
        active.next_controller = next;
        plan.effects.push(ControlEffect::ControllerSelected {
            actor: a.actor,
            life,
            changed: a.dominant.is_some_and(|old| old != life),
        });
        Ok(Some(plan))
    }
    pub fn boundary(
        &self,
        store: &Store,
        now: SimTime,
        limit: SimTime,
        alive: &BTreeSet<PersonId>,
    ) -> SimTime {
        let mut next = limit;
        for (actor, b) in burdens(store) {
            if alive.contains(&actor) && !self.active.as_ref().is_some_and(|a| a.actor == actor) {
                next = recovery_boundary(&self.rules, b, now, next);
            }
        }
        if let Some(a) = &self.active
            && alive.contains(&a.actor)
        {
            let b = burden(store, a.actor).cloned().unwrap_or_default();
            next = active_boundary(
                &self.rules,
                &b,
                ActiveInput {
                    life_count: a.lives.len(),
                    accepted: &a.accepted,
                    episode: a.episode.as_ref(),
                    next_loss_burden: a.next_loss_burden,
                },
                now,
                a.next_controller,
                next,
            );
        }
        next
    }
}
pub fn warning_spec(
    r: &Rules,
    actor: PersonId,
    lives: &BTreeSet<PersonId>,
    req: CostRequirement,
) -> Result<InvocationWarning, Error> {
    let effect = cost_effect(r, lives.len(), req)?;
    Ok(InvocationWarning {id:WarningId(0),actor,lives:lives.clone(),cost:req.cost,severity:req.severity,activation_burden:0,instant_fatigue:0,fatigue_per_mille:effect.fatigue_per_mille,health_harm_per_mille:effect.health_harm_per_mille,identity_skill_penalty:effect.identity_skill_penalty,emotion_disruption:if req.cost==InvocationCost::Emotional {u16::from(req.severity)*100}else{0},lifespan_loss:effect.lifespan_loss,control_loss_duration:if req.cost==InvocationCost::LossOfControl {r.loss_duration}else{SimDuration(0)},permanent:req.permanent,description:match req.cost {
        InvocationCost::Strain=>"Invocation adds the shown fatigue and injury rates while these lives remain active; ordinary rest can recover strain.",
        InvocationCost::Emotional=>"Emotional interference changes judgment and impulsiveness; it recovers as burden falls.",
        InvocationCost::Identity if !req.permanent=>"Temporary identity interference reduces effective mastery until burden recovers.",
        InvocationCost::Identity=>"Very protracted exposure permanently impairs this person's identity and effective capabilities. A successor does not inherit this direct penalty.",
        InvocationCost::Lifespan=>"Very protracted exposure permanently reduces this person's remaining lifespan; already advanced age can make the reduction fatal.",
        InvocationCost::LossOfControl=>"Past lives direct this body for the exact shown duration. Actions and dismissal are unavailable; all ensuing costs, irreversible harm, and death are authorized during it. Pause, observation, and saving remain available.",
    }.into()})
}
