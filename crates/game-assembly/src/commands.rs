//! The coordinator routes actor intentions. Domain adapters do not own the
//! cross-system command dispatcher or authoritative receipt sequence.
use crate::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActivityOwner {
    Agreements,
    Projects,
    Health,
    Property,
    Transport,
    Missions,
    Invocation,
}

impl Simulation {
    pub(crate) fn conversation_actor_available(&self, actor: PersonId) -> bool {
        (self.host.state.physical.is_some()
            && self
                .host
                .storage
                .activity(actor)
                .is_some_and(|activity| matches!(activity, Activity::Idle | Activity::Rest)))
            && self.host.state.armies.as_ref().is_none_or(|w| {
                !w.talks
                    .values()
                    .any(|t| t.speaker == actor || t.listener == actor)
            })
            && self.host.state.missions.as_ref().is_none_or(|_| {
                !self
                    .host
                    .state
                    .missions
                    .as_ref()
                    .unwrap()
                    .actor_talking(&self.host.storage, actor)
            })
            && self.host.state.realm.as_ref().is_none_or(|w| {
                !w.talks.contains_key(&actor) && !w.talks.values().any(|t| t.recipient == actor)
            })
            && !self.property_actor_talking(actor)
            && self
                .host
                .state
                .social
                .as_ref()
                .is_none_or(|w| !w.participants.contains_key(&actor))
    }

    pub(crate) fn submit_actor_action(
        &mut self,
        submission: Submission,
    ) -> Result<CommandReceipt, Error> {
        self.host
            .state
            .next_command
            .checked_add(1)
            .ok_or(Error::Overflow)?;
        self.debug_actor_command(self.host.state.ruler, submission.command.clone())?;
        let receipt = CommandReceipt {
            caller_sequence: submission.caller_sequence,
            authoritative_sequence: self.host.state.next_command,
            accepted_at: self.host.clock().time,
        };
        self.host.state.next_command += 1;
        self.host.state.accepted.insert(
            submission.caller_sequence,
            (submission.command, receipt.clone()),
        );
        Ok(receipt)
    }

    pub fn debug_actor_command(&mut self, actor: PersonId, command: Command) -> Result<(), Error> {
        let revisions = match &command {
            Command::Enforcement { .. }
            | Command::Realm { .. }
            | Command::Social { .. }
            | Command::Transport { .. } => 2,
            _ => 1,
        };
        self.host.preflight_records(0, 0, revisions)?;
        let started = Instant::now();
        self.preflight_agreements()?;
        self.profile
            .record_phase("command.preflight_agreements", started);
        let started = Instant::now();
        self.preflight_institutions()?;
        self.profile
            .record_phase("command.preflight_institutions", started);
        if let Command::Army { ref action } = command {
            self.army_command(actor, action)?;
            self.host.bump_revision(1)?;
            return Ok(());
        }
        if let Command::Realm { ref action } = command {
            self.realm_command(actor, action)?;
            self.host.bump_revision(1)?;
            return Ok(());
        }
        if let Command::Social { ref action } = command {
            self.start_social(actor, action.clone())?;
            self.host.bump_revision(1)?;
            return Ok(());
        }
        if let Command::Enforcement { ref action } = command {
            self.enforcement_command(actor, action)?;
            self.host.bump_revision(1)?;
            return Ok(());
        }
        if let Command::Transport { ref action } = command {
            self.transport_command(actor, action)?;
            self.host.bump_revision(1)?;
            return Ok(());
        }
        if let Command::Health { ref action } = command {
            self.health_command(actor, action)?;
            self.host.bump_revision(1)?;
            return Ok(());
        }
        if let Command::Ecology { ref action } = command {
            return self.debug_ecology_action(actor, action);
        }
        if let Command::Mission { ref action } = command {
            self.mission_command(actor, action)?;
            self.observe_agreement_events();
            self.host.bump_revision(1)?;
            return Ok(());
        }
        match command {
            Command::Escape => return self.start_escape(actor),
            Command::Restrain { target } => return self.start_restraint(actor, target),
            Command::Yield { captor } => return self.force_yield(actor, captor),
            Command::WithdrawYield => return self.force_withdraw_yield(actor),
            Command::FreeCaptive { target } if target == actor => return self.start_escape(actor),
            _ => {}
        }
        if let Command::Institution { ref action } = command {
            self.institution_command(actor, action)?;
            self.reconcile_institutions()?;
            self.host.bump_revision(1)?;
            return Ok(());
        }
        if let Command::Invocation { ref action } = command {
            self.invocation_command(actor, action)?;
            self.update_property_events()?;
            self.reconcile_agreements()?;
            self.observe_agreement_events();
            self.host.bump_revision(1)?;
            return Ok(());
        }
        if let Command::Carry { source, quantity } = command {
            return self.carry_good(actor, source, quantity);
        }
        if let Command::Project { action } = command {
            return self.debug_project_action(actor, action);
        }
        if let Command::Learn { teacher, topic } = command {
            return self.start_lesson(actor, teacher, topic);
        }
        if let Command::RequestGuardian { guardian, request } = command {
            return self.request_guardian(actor, guardian, request);
        }
        if let Command::Property { action } = command {
            self.debug_property_action(actor, action)?;
            self.update_property_events()?;
            self.reconcile_agreements()?;
            self.observe_agreement_events();
            return Ok(());
        }
        if let Command::Agreement { ref action } = command {
            self.agreement_command(actor, action)?;
            self.observe_agreement_events();
            self.host.bump_revision(1)?;
            return Ok(());
        }
        if let Command::Cultivate { field, operation } = command {
            return self.start_cultivation(actor, field, operation);
        }
        if let Command::Conceive { partner } = command {
            return self.start_conception(actor, partner);
        }
        self.start_physical_action(actor, command)
    }
    pub(crate) fn interrupt_actor_activity(&mut self, actor: PersonId) {
        self.interrupt_actor_activity_preserving(actor, None);
    }

    #[cfg(test)]
    pub(crate) fn interrupt_actor_activity_except_projects(&mut self, actor: PersonId) {
        self.interrupt_actor_activity_preserving(actor, Some(ActivityOwner::Projects));
    }

    pub(crate) fn interrupt_actor_activity_except_health(&mut self, actor: PersonId) {
        self.interrupt_actor_activity_preserving(actor, Some(ActivityOwner::Health));
    }

    pub(crate) fn interrupt_actor_activity_except_property(&mut self, actor: PersonId) {
        self.interrupt_actor_activity_preserving(actor, Some(ActivityOwner::Property));
    }

    fn interrupt_actor_activity_preserving(
        &mut self,
        actor: PersonId,
        owner: Option<ActivityOwner>,
    ) {
        self.interrupt_army_talk(actor);
        self.interrupt_realm(actor);
        self.interrupt_transport_lift(actor);
        self.interrupt_social(actor);
        self.interrupt_enforcement(actor);
        if owner != Some(ActivityOwner::Health) {
            self.cancel_health_activity(actor);
        }
        self.interrupt_ecology(actor);
        self.cancel_mission_talk(actor);
        self.interrupt_force(actor);
        self.cancel_invocation_action(actor);
        if owner != Some(ActivityOwner::Projects) {
            self.interrupt_construction(actor);
        }
        self.interrupt_lesson(actor);
        if owner != Some(ActivityOwner::Property) {
            self.interrupt_property_action(actor);
        }
        self.interrupt_cultivation(actor);
        self.invalidate_pregnancies(actor, false);
        if self.host.state.physical.is_some() {
            if let Activity::Care { child, .. } | Activity::Comfort { child, .. } =
                *self.host.storage.activity(actor).expect("activity exists")
            {
                self.care_index.remove(&child);
            }
            *self
                .host
                .storage
                .activity_mut(actor)
                .expect("activity exists") = Activity::Idle;
            for due in self.host.state.due.values_mut() {
                due.retain(|d| !matches!(d, Due::Physical(id) if *id == actor));
            }
            self.host.state.due.retain(|_, due| !due.is_empty());
        }
    }
}
