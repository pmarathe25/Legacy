//! Service-owned iteration and progression over the canonical store.
//! Ports expose facts, prepared publication and other owners' command entry points.
use crate::*;
use game_core::*;
use game_ecs::Store;
use std::collections::BTreeSet;

pub trait MissionExecution {
    fn runtime(&self) -> &Runtime;
    fn store(&self) -> &Store;
    fn parts_mut(&mut self) -> (&mut Runtime, &mut Store);
    fn now(&self) -> SimTime;
    fn presence(&self) -> impl PresenceFacts + CommandAvailabilityFacts;
    fn membership_index(&self) -> &MembershipIndex;
    fn player(&self) -> PersonId;
    fn preflight_review(&self) -> Result<(), Error>;
    fn recruitment_input(&self, actor: PersonId, mission: &Mission) -> RecruitmentInput;
    fn personal_inventory(&self, actor: PersonId) -> InventoryId;
    fn known_stocks(&self, actor: PersonId) -> Vec<StockKnowledge>;
    fn command(&mut self, actor: PersonId, action: &MissionAction) -> Result<(), Error>;
    fn control_member(&mut self, mission: MissionId, actor: PersonId);
    fn commit_plan(&mut self, plan: PreparedMission) -> Result<(), Error>;
    fn observation_facts(&self, actor: PersonId, mission: MissionId) -> ObservationFacts;
    fn member_body(&self, person: PersonId) -> MemberBody;
    fn objective_knowledge(
        &self,
        actor: PersonId,
        objective: &MissionObjective,
    ) -> ObjectiveKnowledge;
    fn talk_scene(&self, speaker: PersonId, listener: PersonId) -> TalkScene;
    fn order_authority(&self, speaker: PersonId, order: &CommunicatedOrder)
    -> Option<CommandFacts>;
    fn commit_talk(&mut self, plan: PreparedTalkCompletion) -> Result<Vec<MissionEvent>, Error>;
    fn publish_talk_events(&mut self, events: Vec<MissionEvent>);
    fn prepare_order(&self, order: &CommunicatedOrder) -> Result<PreparedMission, Error>;
}

pub fn observe_member(port: &mut impl MissionExecution, actor: PersonId, id: MissionId) {
    let facts = port.observation_facts(actor, id);
    if let Ok(plan) = port
        .runtime()
        .prepare_observation(port.store(), actor, id, &facts)
    {
        plan.commit(port.parts_mut().1)
            .expect("same personal observation snapshot");
    }
}
pub fn publish_mission(port: &mut impl MissionExecution, id: MissionId) {
    let recipients = port.runtime().receivers(port.store(), id, &port.presence());
    for person in recipients {
        observe_member(port, person, id);
    }
}
pub fn reconcile_outcome(port: &mut impl MissionExecution, id: MissionId) -> Result<(), Error> {
    let mission = mission(port.store(), id).ok_or(Error::Unavailable)?;
    let bodies = mission
        .members
        .keys()
        .map(|p| (*p, port.member_body(*p)))
        .collect();
    let leader_knowledge =
        port.objective_knowledge(mission.proposal.leader, &mission.proposal.objective);
    let receivers = port
        .runtime()
        .receivers(port.store(), id, &port.presence())
        .into_iter()
        .collect();
    let plan = port.runtime().prepare_outcome(
        port.store(),
        id,
        &OutcomeFacts {
            now: port.now(),
            bodies,
            leader_knowledge,
            receivers,
        },
    )?;
    port.commit_plan(plan)
}

/// External events have already been translated to typed facts. Arrival
/// visibility is resolved by service policy at the point that receipt is read.
pub enum ReceiptInput {
    Receipt(MissionReceipt),
    Arrived {
        actor: PersonId,
        area: AreaId,
        contacts: BTreeSet<PersonId>,
    },
}
pub fn reconcile_receipts(
    port: &mut impl MissionExecution,
    events: impl IntoIterator<Item = (EventId, ReceiptInput)>,
) -> Result<(), Error> {
    let observers = port.runtime().receipt_observers(port.store());
    for (sequence, event) in events {
        let receipt = match event {
            ReceiptInput::Receipt(receipt) => receipt,
            ReceiptInput::Arrived {
                actor,
                area,
                contacts,
            } => observers.arrived(port.runtime(), actor, area, contacts, &port.presence()),
        };
        let plan = port
            .runtime()
            .prepare_receipt(port.store(), sequence, &receipt)?;
        let (runtime, store) = port.parts_mut();
        plan.commit(runtime, store)?;
    }
    Ok(())
}
pub fn reconcile_progress(port: &mut impl MissionExecution) -> Result<(), Error> {
    let speakers = port.runtime().due_talks(port.store(), port.now());
    for speaker in speakers {
        let listener = talk(port.store(), speaker)
            .ok_or(Error::Unavailable)?
            .listener;
        let order = port.runtime().completion_order(port.store(), speaker);
        let authority = order
            .as_ref()
            .and_then(|order| port.order_authority(speaker, order));
        let plan = port.runtime().prepare_talk_completion(
            port.store(),
            speaker,
            port.now(),
            port.talk_scene(speaker, listener),
            authority.as_ref(),
        )?;
        let order = plan.order.clone();
        let events = port.commit_talk(plan)?;
        if let Some(order) = order
            && let Ok(plan) = port.prepare_order(&order)
        {
            port.commit_plan(plan)?;
        }
        port.publish_talk_events(events);
    }
    let ids = missions(port.store()).map(|(id, _)| id).collect::<Vec<_>>();
    for id in ids {
        reconcile_outcome(port, id)?;
        let actors = mission(port.store(), id)
            .ok_or(Error::Unavailable)?
            .members
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for actor in actors {
            observe_member(port, actor, id);
        }
        publish_mission(port, id);
    }
    Ok(())
}
pub fn review_missions(port: &mut impl MissionExecution) -> Result<(), Error> {
    let now = port.now();
    if !port.runtime().review_due(now) {
        return Ok(());
    }
    port.preflight_review()?;
    port.parts_mut().0.reviewed(now);
    let offers = port
        .runtime()
        .recruitment_reviews(port.store(), port.player(), &port.presence());
    for id in offers {
        let offer = recruitment(port.store(), id).ok_or(Error::Unavailable)?;
        let mission = mission(port.store(), offer.mission).ok_or(Error::Unavailable)?;
        let actor = offer.person;
        let input = port.recruitment_input(actor, mission);
        let Some(area) = PresenceFacts::area(&port.presence(), actor) else {
            continue;
        };
        let known = port.known_stocks(actor);
        let action = port.runtime().recruitment_response(
            port.store(),
            id,
            input,
            area,
            port.personal_inventory(actor),
            &known,
        );
        if let Some(action) = action {
            // An unavailable material promise remains unanswered.
            let _ = port.command(actor, &action);
        }
    }
    let ids = missions(port.store()).map(|(id, _)| id).collect::<Vec<_>>();
    for id in ids {
        if let Some(leader) = port
            .runtime()
            .autonomous_departure(port.store(), id, port.player())
        {
            let _ = port.command(leader, &MissionAction::Depart { mission: id });
        }
        let actors = port
            .runtime()
            .control_members(port.store(), port.membership_index(), id);
        for actor in actors {
            port.control_member(id, actor);
        }
    }
    Ok(())
}

pub trait MemberExecution<A> {
    fn action(&mut self, mission: MissionId, actor: PersonId, action: &MemberAction<A>) -> bool;
    fn explain(&mut self, mission: MissionId, actor: PersonId, explanation: MemberExplanation);
}
pub fn execute_member_decision<A>(
    port: &mut impl MemberExecution<A>,
    mission: MissionId,
    actor: PersonId,
    decision: MemberDecision<A>,
) {
    for attempt in decision.attempts {
        let success = port.action(mission, actor, &attempt.action);
        if let Some(followup) = attempt.after_result(success) {
            match followup {
                Failure::Action(action) => {
                    port.action(mission, actor, &action);
                }
                Failure::Explain(explanation) => port.explain(mission, actor, explanation),
            }
        }
    }
    if let Some(explanation) = decision.explanation {
        port.explain(mission, actor, explanation);
    }
}

pub fn publish_notices(
    port: &mut impl MissionExecution,
    notices: impl IntoIterator<Item = MissionId>,
) {
    for mission in notices {
        publish_mission(port, mission);
    }
}
pub fn react_to_encounter_end(port: &mut impl MissionExecution, encounter: EncounterId) {
    let Some(encounter) = game_conflict::encounter(port.store(), encounter) else {
        return;
    };
    let responses = port
        .runtime()
        .encounter_end_responses(port.store(), encounter);
    for (mission, actor) in responses {
        port.control_member(mission, actor);
    }
}
