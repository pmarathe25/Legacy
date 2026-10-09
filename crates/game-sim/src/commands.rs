//! The coordinator routes actor intentions. Domain adapters do not own the
//! cross-system command dispatcher or authoritative receipt sequence.
use crate::*;

impl Simulation {
    pub(crate) fn submit_actor_action(
        &mut self,
        submission: Submission,
    ) -> Result<CommandReceipt, Error> {
        self.state
            .next_command
            .checked_add(1)
            .ok_or(Error::Overflow)?;
        self.debug_actor_command(self.state.ruler, submission.command.clone())?;
        let receipt = CommandReceipt {
            caller_sequence: submission.caller_sequence,
            authoritative_sequence: self.state.next_command,
            accepted_at: self.state.time,
        };
        self.state.next_command += 1;
        self.state.accepted.insert(
            submission.caller_sequence,
            (submission.command, receipt.clone()),
        );
        Ok(receipt)
    }

    pub fn debug_actor_command(&mut self, actor: PersonId, command: Command) -> Result<(), Error> {
        self.preflight_agreements()?;
        self.preflight_institutions()?;
        if let Command::Social { ref action } = command {
            self.start_social(actor, action.clone())?;
            self.revision += 1;
            return Ok(());
        }
        if let Command::Enforcement { ref action } = command {
            self.enforcement_command(actor, action)?;
            self.revision += 1;
            return Ok(());
        }
        if let Command::Health { ref action } = command {
            self.health_command(actor, action)?;
            self.revision += 1;
            return Ok(());
        }
        if let Command::Ecology { ref action } = command {
            return self.debug_ecology_action(actor, action);
        }
        if let Command::Mission { ref action } = command {
            self.mission_command(actor, action)?;
            self.observe_agreement_events();
            self.revision += 1;
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
            self.revision += 1;
            return Ok(());
        }
        if let Command::Invocation { ref action } = command {
            self.invocation_command(actor, action)?;
            self.update_property_events();
            self.reconcile_agreements()?;
            self.observe_agreement_events();
            self.revision += 1;
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
            self.update_property_events();
            self.reconcile_agreements()?;
            self.observe_agreement_events();
            return Ok(());
        }
        if let Command::Agreement { ref action } = command {
            self.agreement_command(actor, action)?;
            self.observe_agreement_events();
            self.revision += 1;
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
        self.interrupt_social(actor);
        self.interrupt_enforcement(actor);
        self.cancel_health_activity(actor);
        self.interrupt_ecology(actor);
        self.cancel_mission_talk(actor);
        self.interrupt_force(actor);
        self.cancel_invocation_action(actor);
        self.interrupt_construction(actor);
        self.interrupt_lesson(actor);
        self.interrupt_property_action(actor);
        self.interrupt_cultivation(actor);
        self.invalidate_pregnancies(actor, false);
        if let Some(world) = &mut self.state.physical {
            let a = world.actors.get_mut(&actor).unwrap();
            if let Activity::Care { child, .. } | Activity::Comfort { child, .. } = a.activity {
                self.care_index.remove(&child);
            }
            a.activity = Activity::Idle;
            for due in self.state.due.values_mut() {
                due.retain(|d| !matches!(d, Due::Physical(id) if *id == actor));
            }
            self.state.due.retain(|_, due| !due.is_empty());
        }
    }
}
