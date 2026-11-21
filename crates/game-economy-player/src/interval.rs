//! Player-controlled actions on the interval host, without administrative events.
use game_economy::core_interval::{
    CoreHost, CoreMarkInscription, CoreMarkObservation, CorePlayerView, CorePromiseReply,
    MechanicalApparatus, MechanicalExperiment, MechanicalObservation, WoundTreatment,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Advance {
    pub elapsed_millis: u64,
    pub boundaries: u64,
    /// Earlier elapsed time stays committed if a later stage fails.
    pub failure: Option<String>,
}

/// One explicitly controlled person; every action rechecks current control.
pub struct Player<'a> {
    host: &'a mut CoreHost,
    person: u64,
}

impl<'a> Player<'a> {
    pub fn new(host: &'a mut CoreHost, person: u64) -> Result<Self, String> {
        host.controlled_person(person)?;
        Ok(Self { host, person })
    }

    pub fn view(&self) -> Result<CorePlayerView, String> {
        self.host.player_view(self.person)
    }

    /// Advance time without exposing other people's private decision events.
    pub fn advance(&mut self, millis: u64) -> Result<Advance, String> {
        self.host.controlled_person(self.person)?;
        let run = self.host.advance_continuous(millis, &[])?;
        Ok(Advance {
            elapsed_millis: run.elapsed_millis,
            boundaries: run.boundaries,
            failure: run.failure,
        })
    }

    pub fn depart(&mut self, destination: u64) -> Result<bool, String> {
        self.host.controlled_person(self.person)?;
        let result = self.host.depart(
            &[game_economy::Departure {
                person: self.person,
                destination,
            }],
            &[],
        )?;
        Ok(result.first().is_some_and(|outcome| outcome.departed))
    }

    pub fn gather(&mut self, node: u64) -> Result<Option<game_economy::GatheredResource>, String> {
        self.host.gather_controlled(self.person, node)
    }

    pub fn promise(
        &mut self,
        recipient: u64,
        good: u32,
        quantity: u64,
        due_in_millis: u64,
    ) -> Result<u64, String> {
        self.host
            .promise_controlled(self.person, recipient, good, quantity, due_in_millis)
    }

    pub fn respond_to_promise(
        &mut self,
        contract: u64,
        reply: CorePromiseReply,
    ) -> Result<(), String> {
        self.host
            .respond_to_controlled_promise(self.person, contract, reply)
    }

    pub fn deliver_promise(&mut self, contract: u64, quantity: u64) -> Result<bool, String> {
        self.host
            .deliver_controlled_promise(self.person, contract, quantity)
    }

    pub fn assemble(&mut self, apparatus: MechanicalApparatus) -> Result<bool, String> {
        self.host
            .assemble_controlled_apparatus(self.person, apparatus)
    }

    pub fn experiment(
        &mut self,
        request: MechanicalExperiment,
    ) -> Result<MechanicalObservation, String> {
        if request.actor != self.person {
            return Err("experiment actor is outside your control".into());
        }
        self.host.experiment_controlled_apparatus(request)
    }

    pub fn walk(
        &mut self,
        destination: game_economy::LocalEndpoint,
    ) -> Result<game_economy::LocalWalkOutcome, String> {
        self.host.walk_controlled(self.person, destination)
    }

    pub fn pause_walk(&mut self, paused: bool) -> Result<bool, String> {
        self.host.pause_controlled_walk(self.person, paused)
    }

    pub fn plan_expedition(
        &mut self,
        destination: u64,
        visit_millis: u64,
        objective: Option<game_economy::MissionObjective>,
    ) -> Result<game_economy::core_interval::ExpeditionEvent, String> {
        self.host
            .plan_controlled_expedition(self.person, destination, visit_millis, objective)
    }

    pub fn expedition_invitations(
        &self,
    ) -> Result<Vec<game_economy::core_interval::ExpeditionInvitation>, String> {
        self.host.expedition_invitations(self.person)
    }

    pub fn invite_expedition(
        &mut self,
        target: u64,
    ) -> Result<game_economy::core_interval::CoreExpeditionOffer, String> {
        self.host.invite_controlled_expedition(self.person, target)
    }

    pub fn reply_to_expedition(
        &mut self,
        invitation: u64,
        join: bool,
    ) -> Result<game_economy::core_interval::ExpeditionReply, String> {
        self.host
            .reply_controlled_expedition(self.person, invitation, join)
    }

    pub fn withdraw_expedition_invitation(
        &mut self,
        invitation: u64,
    ) -> Result<game_economy::core_interval::ExpeditionReply, String> {
        self.host
            .withdraw_controlled_expedition_invitation(self.person, invitation)
    }

    pub fn schedule_expedition_departure(
        &mut self,
        delay_millis: u64,
    ) -> Result<game_economy::core_interval::ExpeditionEvent, String> {
        self.host
            .schedule_controlled_departure(self.person, delay_millis)
    }

    pub fn continue_expedition(
        &mut self,
    ) -> Result<Vec<game_economy::core_interval::ExpeditionEvent>, String> {
        self.host.continue_controlled_expedition(self.person)
    }

    pub fn return_from_expedition(
        &mut self,
    ) -> Result<Vec<game_economy::core_interval::ExpeditionEvent>, String> {
        self.host.return_controlled_expedition(self.person)
    }

    pub fn survey_expedition(
        &mut self,
    ) -> Result<Vec<game_economy::core_interval::ExpeditionEvent>, String> {
        self.host.survey_controlled_expedition(self.person)
    }

    pub fn abandon_expedition(
        &mut self,
    ) -> Result<game_economy::core_interval::ExpeditionEvent, String> {
        self.host.abandon_controlled_expedition(self.person)
    }

    pub fn confine(&mut self, target: u64, site: u64) -> Result<bool, String> {
        self.host
            .confine_controlled_target(self.person, target, site)
    }

    pub fn confront(
        &mut self,
        target: u64,
    ) -> Result<game_economy::core_interval::TacticalEvent, String> {
        self.host.start_controlled_engagement(self.person, target)
    }

    pub fn choose_tactics(
        &mut self,
        choice: game_economy::core_interval::TacticalChoice,
    ) -> Result<(), String> {
        self.host.choose_controlled_tactics(self.person, choice)
    }

    pub fn guard(&mut self, site: u64) -> Result<(), String> {
        self.host.guard_controlled_cell(self.person, site)
    }

    pub fn release(&mut self, target: u64) -> Result<bool, String> {
        self.host.release_controlled_target(self.person, target)
    }

    pub fn escape(&mut self) -> Result<bool, String> {
        self.host.attempt_controlled_escape(self.person)
    }

    pub fn build(&mut self, blueprint: u32) -> Result<Option<u64>, String> {
        self.host.build_controlled_site(self.person, blueprint)
    }

    pub fn demolish(&mut self, site: u64) -> Result<bool, String> {
        self.host.demolish_controlled_site(self.person, site)
    }

    pub fn salvage(&mut self, site: u64, good: u32, quantity: u64) -> Result<bool, String> {
        self.host
            .salvage_controlled_site(self.person, site, good, quantity)
    }

    pub fn eat(&mut self, good: u32) -> Result<bool, String> {
        self.host.controlled_person(self.person)?;
        self.host.eat_controlled(self.person, good)
    }

    pub fn choose_clothing(&mut self, good: Option<u32>) -> Result<(), String> {
        self.host.choose_controlled_clothing(self.person, good)
    }

    pub fn set_rest(&mut self, resting: bool) -> Result<(), String> {
        self.host.set_controlled_rest(self.person, resting)
    }

    pub fn choose_written_form(&mut self, sounds: Vec<u8>, marks: Vec<u32>) -> Result<(), String> {
        self.host.controlled_person(self.person)?;
        self.host.choose_written_form(
            self.person,
            game_language::writing::Association { sounds, marks },
        )
    }

    pub fn choose_word(
        &mut self,
        meaning: game_language::Meaning,
        sounds: Vec<u8>,
    ) -> Result<(), String> {
        self.host
            .choose_controlled_word(self.person, game_language::Word { meaning, sounds })
    }

    pub fn inscribe(&mut self, sounds: Vec<u8>) -> Result<Option<u64>, String> {
        self.host.controlled_person(self.person)?;
        let result = self.host.inscribe_marks(&[CoreMarkInscription {
            person: self.person,
            sounds,
        }])?;
        Ok(result.first().and_then(|row| row.1))
    }

    pub fn observe_marked_page(&self, item: u64) -> Result<CoreMarkObservation, String> {
        self.host.controlled_person(self.person)?;
        self.host.observe_marked_page(self.person, item)
    }

    pub fn adopt_remembered_form(&mut self, page: u64) -> Result<(), String> {
        let view = self.view()?;
        let memory = view
            .language
            .as_ref()
            .and_then(|language| language.observations.iter().find(|row| row.page == page))
            .ok_or("the controlled person has no such observation")?;
        let sounds = memory
            .heard
            .clone()
            .ok_or("the controlled person did not hear a sound")?;
        let form = game_language::writing::Association {
            sounds,
            marks: memory.marks.clone(),
        };
        self.host
            .choose_remembered_forms(&[(self.person, page, form)])
    }

    pub fn start_work(&mut self, recipe: u32, units: u32) -> Result<Option<u64>, String> {
        self.host.controlled_person(self.person)?;
        let (job, accepted) = self
            .host
            .start_controlled_work(self.person, recipe, units)?;
        Ok(accepted.then_some(job))
    }

    pub fn stop_work(&mut self, job: u64) -> Result<(), String> {
        let person = self.host.controlled_person(self.person)?;
        if person.health == 0 {
            return Err("the controlled person cannot stop work while incapacitated".into());
        }
        self.host.abandon_work(&[(job, self.person)])
    }

    pub fn stop_assisting(&mut self) -> Result<(), String> {
        let person = self.host.controlled_person(self.person)?;
        if person.health == 0 {
            return Err("the controlled person cannot stop assisting while incapacitated".into());
        }
        self.host.withdraw_shared_helpers(&[self.person])
    }

    pub fn treat_wound(&mut self, good: u32) -> Result<bool, String> {
        self.host.controlled_person(self.person)?;
        let result = self.host.treat_wounds(
            &[WoundTreatment {
                person: self.person,
                good,
            }],
            &[],
        )?;
        Ok(result
            .outcomes
            .first()
            .is_some_and(|outcome| outcome.accepted))
    }
}
