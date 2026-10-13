//! The coordinator routes actor intentions. Domain adapters do not own the
//! cross-system command dispatcher or authoritative receipt sequence.
use crate::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActivityOwner {
    All,
    Agreements,
    Projects,
    Agriculture,
    Ecology,
    Enforcement,
    Combat,
    Force,
    Physical,
    Health,
    Property,
    Transport,
    Missions,
    Invocation,
    Realm,
}

impl Simulation {
    pub(crate) fn conversation_actor_available(&self, actor: PersonId) -> bool {
        use game_characters::orchestration::{ConversationActivity, ConversationAvailability};
        ConversationAvailability {
            physical_enabled: self.host.state.physical.is_some(),
            activity: self
                .host
                .storage
                .activity(actor)
                .map(|activity| match activity {
                    Activity::Idle => ConversationActivity::Idle,
                    Activity::Rest => ConversationActivity::Rest,
                    _ => ConversationActivity::Other,
                }),
            army_talking: self.host.state.armies.as_ref().is_some_and(|_| {
                game_armies::talks(&self.host.storage)
                    .map(|(_, talk)| talk)
                    .any(|t| t.speaker == actor || t.listener == actor)
            }),
            mission_talking: self
                .host
                .state
                .missions
                .as_ref()
                .is_some_and(|world| world.actor_talking(&self.host.storage, actor)),
            realm_busy: self
                .host
                .state
                .realm
                .as_ref()
                .is_some_and(|w| w.actor_busy(&self.host.storage, actor)),
            property_talking: self.property_actor_talking(actor),
            social_participant: self.host.state.social.as_ref().is_some_and(|_| {
                game_characters::social::participant(&self.host.storage, actor).is_some()
            }),
        }
        .available()
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
            return self.routed_institution_command(actor, action);
        }
        if let Command::Invocation { ref action } = command {
            return self.invocation_command(actor, action);
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
            return self.property_command(actor, action);
        }
        if let Command::Agreement { ref action } = command {
            return self.agreement_command_with_revision(actor, action, 1);
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

    pub(crate) fn try_interrupt_actor_activity(&mut self, actor: PersonId) -> Result<(), Error> {
        self.try_interrupt_actor_activity_preserving(actor, None)
    }

    #[cfg(test)]
    pub(crate) fn interrupt_actor_activity_except_projects(&mut self, actor: PersonId) {
        self.interrupt_actor_activity_preserving(actor, Some(ActivityOwner::Projects));
    }

    fn interrupt_actor_activity_preserving(
        &mut self,
        actor: PersonId,
        owner: Option<ActivityOwner>,
    ) {
        self.try_interrupt_actor_activity_preserving(actor, owner)
            .expect("preflighted actor interruption");
    }

    fn try_interrupt_actor_activity_preserving(
        &mut self,
        actor: PersonId,
        owner: Option<ActivityOwner>,
    ) -> Result<(), Error> {
        let opening = self.host.transaction_opening();
        let prepared = self.prepare_interruptions(&[actor], owner.unwrap_or(ActivityOwner::All))?;
        let mut batch = game_ecs::Batch::new();
        let mut finish = prepared.stage_into(self, &mut batch, &BTreeMap::new())?;
        finish.validate_batch(self, &batch)?;
        let mut transaction = game_world::Transaction::new(opening, batch);
        transaction.events = finish.events_for(&[actor]);
        self.host.commit_transaction(transaction)?;
        finish.finish_actor_metadata(self, actor);
        Ok(())
    }
}
