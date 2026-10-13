//! Stage one global boundary before publishing its clock or closing effects.
//! Owners retain the existing ordered preparation and receiving entry points.
use crate::*;
use game_ecs::{Batch, Schema, Store};

impl Simulation {
    /// Application/runtime inputs exclude the unrelated client receipt ledger.
    pub(crate) fn interval_inputs(&self) -> AssemblyState {
        let state = &self.host.state;
        AssemblyState {
            openings: state.openings.clone(),
            workshops: state.workshops.clone(),
            armies: state.armies.clone(),
            realm: state.realm.clone(),
            transport: state.transport.clone(),
            genetics: state.genetics.clone(),
            physical_traits: state.physical_traits.clone(),
            temperament: state.temperament.clone(),
            heredity: state.heredity.clone(),
            content: state.content.clone(),
            scenario: state.scenario.clone(),
            ruler: state.ruler,
            areas: state.areas.clone(),
            physical: state.physical.clone(),
            decisions: state.decisions.clone(),
            development: state.development.clone(),
            combat: state.combat.clone(),
            environment: state.environment.clone(),
            agriculture: state.agriculture.clone(),
            agreements: state.agreements.clone(),
            lineage: state.lineage.clone(),
            property: state.property.clone(),
            institutions: state.institutions.clone(),
            skills: state.skills.clone(),
            projects: state.projects.clone(),
            invocation: state.invocation.clone(),
            force: state.force.clone(),
            missions: state.missions.clone(),
            ecology: state.ecology.clone(),
            health: state.health.clone(),
            social: state.social.clone(),
            enforcement: state.enforcement.clone(),
            due: state.due.clone(),
            accepted: BTreeMap::new(),
            next_command: state.next_command,
        }
    }

    fn interval_event_tail(
        &self,
        prepared: &engine_adapter::PreparedWorldStep,
    ) -> Vec<game_world::Published<EventKind>> {
        let state = &self.host.state;
        let mut cursors = Vec::new();
        if let Some(runtime) = &state.lineage
            && let Some(cursor) = runtime.delivery_receipt_cursor(&self.host.storage)
        {
            cursors.push(cursor);
        }
        if let Some(runtime) = &state.decisions {
            cursors.push(runtime.last_learning_event);
        }
        if let Some(runtime) = &state.realm {
            cursors.push(runtime.last_event);
        }
        if let Some(runtime) = &state.agreements {
            cursors.extend([runtime.last_event, runtime.last_observation_event]);
        }
        if let Some(runtime) = &state.property {
            cursors.push(runtime.last_event);
        }
        if let Some(runtime) = &state.institutions {
            cursors.push(runtime.last_event);
        }
        if let Some(runtime) = &state.projects {
            cursors.push(runtime.last_event);
        }
        if let Some(runtime) = &state.force {
            cursors.push(runtime.last_event);
        }
        if let Some(runtime) = &state.missions {
            cursors.push(runtime.last_event);
        }
        if let Some(runtime) = &state.enforcement {
            cursors.push(runtime.last_event);
        }
        let cursor = cursors
            .into_iter()
            .min()
            .unwrap_or(self.host.next_event() - 1);
        let mut records: BTreeMap<_, _> = self
            .host
            .events_since(cursor)
            .iter()
            .map(|event| (event.sequence, event.clone()))
            .collect();
        let mut arrivals: BTreeSet<_> = prepared
            .due
            .iter()
            .filter_map(|due| match due {
                Due::Arrive(actor) => Some(*actor),
                _ => None,
            })
            .collect();
        // Same-timestamp intake and arrival evidence is independent of cursors.
        for event in self
            .host
            .events()
            .iter()
            .rev()
            .take_while(|event| event.at == self.host.clock().time)
        {
            records.insert(event.sequence, event.clone());
            if prepared.next == event.at
                && let EventKind::Arrived { actor, .. } = event.event
            {
                arrivals.insert(actor);
            }
        }
        // Keep only matching old departure evidence, not the traversed history.
        if !arrivals.is_empty() {
            for event in self.host.events().iter().rev() {
                if let EventKind::Departed { actor, arrives, .. } = event.event
                    && arrives == prepared.next
                    && arrivals.contains(&actor)
                {
                    records.insert(event.sequence, event.clone());
                }
            }
        }
        records.into_values().collect()
    }

    fn interval_facade(&self, mut host: GameHost) -> Self {
        // Query handles are store-affine; only plain derived ID indexes copy.
        let agriculture_queries = game_agriculture::Runtime::bind(&mut host.storage);
        let physical_query = host
            .storage
            .query2_optional::<game_physiology::Body, Activity, game_workshop::Job>();
        let environment_queries = game_environment::Runtime::bind(&mut host.storage);
        let ecology_queries = game_ecology::Runtime::bind(&mut host.storage);
        Self {
            host,
            engine: self.engine.clone(),
            execution: self.execution.clone(),
            profile: self.profile.clone(),
            person_ids: self.person_ids.clone(),
            person_index: self.person_index.clone(),
            inventory_index: self.inventory_index.clone(),
            pregnancy_index: self.pregnancy_index.clone(),
            care_index: self.care_index.clone(),
            dependent_index: self.dependent_index.clone(),
            agriculture_queries,
            physical_query,
            environment_queries,
            ecology_queries,
            mission_membership: self.mission_membership.clone(),
        }
    }

    pub(super) fn commit_prepared_step(
        &mut self,
        mut prepared: engine_adapter::PreparedWorldStep,
    ) -> Result<(), Error> {
        // Preserve the established owner error for stale physiological frames.
        self.validate_physical_output(&prepared.physical_output)?;
        if prepared.opening != self.host.transaction_opening()
            || prepared.opening_inputs != self.interval_inputs()
            || !prepared
                .opening_rows
                .schema()
                .same_registrations(self.host.storage.schema())
            || prepared.opening_rows != *self.host.storage
        {
            return Err(Error::InvalidSave("stale world interval inputs".into()));
        }
        let preparation_started = Instant::now();
        let tail = self.interval_event_tail(&prepared);
        let opening = prepared.opening.clone();
        let baseline = std::mem::replace(
            &mut prepared.opening_rows,
            Store::with_schema(Schema::new()),
        );
        let inputs = std::mem::replace(&mut prepared.opening_inputs, self.interval_inputs());
        let host = game_world::GenericHost::preview_with_tail(
            Components::from(baseline.clone()),
            inputs,
            &opening,
            self.host.seed(),
            &tail,
        )?;
        let mut preview = self.interval_facade(host);
        let preparation_ns = preparation_started.elapsed().as_nanos();
        preview.commit_prepared_step_inner(prepared)?;
        if !preview.host.state.accepted.is_empty()
            || preview.host.state.next_command != self.host.state.next_command
        {
            return Err(Error::InvalidConfiguration(
                "interval changed client receipt bookkeeping".into(),
            ));
        }
        let publication_started = Instant::now();
        let mut batch = Batch::new();
        let (reads, footprint) = preview.host.storage.diff_into(&baseline, &mut batch)?;
        let guard = reads.bind(&self.host.storage, &batch, &footprint)?;
        let mut transaction = game_world::Transaction::new(opening.clone(), batch);
        transaction.time = preview.host.clock().time;
        transaction.ids = preview
            .host
            .next_id()
            .checked_sub(opening.next_id)
            .ok_or(Error::Overflow)?;
        transaction.revisions = preview
            .host
            .clock()
            .revision
            .checked_sub(opening.clock.revision)
            .ok_or(Error::Overflow)?;
        let new_records = preview.host.events_since(opening.next_event - 1);
        if new_records.iter().any(|event| event.at != transaction.time) {
            return Err(Error::InvalidConfiguration(
                "interval records span different clocks".into(),
            ));
        }
        transaction.events = new_records
            .iter()
            .map(|record| record.event.clone())
            .collect();
        transaction.guards.push(guard);
        self.host.commit_transaction(transaction)?;
        // All remaining changes are admitted metadata and derived plain indexes.
        preview.host.state.accepted = std::mem::take(&mut self.host.state.accepted);
        self.host.state = preview.host.state;
        self.person_ids = preview.person_ids;
        self.person_index = preview.person_index;
        self.inventory_index = preview.inventory_index;
        self.pregnancy_index = preview.pregnancy_index;
        self.care_index = preview.care_index;
        self.dependent_index = preview.dependent_index;
        self.mission_membership = preview.mission_membership;
        self.profile = preview.profile;
        let publication_ns = publication_started.elapsed().as_nanos();
        self.profile.commit_ns = self
            .profile
            .commit_ns
            .saturating_add(preparation_ns)
            .saturating_add(publication_ns);
        let phase = self
            .profile
            .phases_ns
            .entry("transaction.prepare_interval")
            .or_default();
        *phase = phase.saturating_add(preparation_ns);
        let phase = self
            .profile
            .phases_ns
            .entry("transaction.publish_interval")
            .or_default();
        *phase = phase.saturating_add(publication_ns);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn moving_kernel() -> Simulation {
        let mut sim = Simulation::kernel_fixture(701, 15, Execution::default()).unwrap();
        sim.submit(Submission {
            caller_sequence: 1,
            command: Command::Move {
                destination: AreaId(2),
            },
        })
        .unwrap();
        sim
    }

    #[test]
    fn staging_preserves_closing_order_arrival_evidence_and_client_receipts() {
        let mut staged = moving_kernel();
        let mut sequential = moving_kernel();
        for at in [SimTime(60_000), SimTime(120_000)] {
            let prepared = staged.prepare_world_step(at).unwrap();
            staged.commit_prepared_step(prepared).unwrap();
            let prepared = sequential.prepare_world_step(at).unwrap();
            sequential.commit_prepared_step_inner(prepared).unwrap();
            assert_eq!(
                staged.canonical_snapshot().unwrap(),
                sequential.canonical_snapshot().unwrap()
            );
            assert_eq!(staged.host.clock(), sequential.host.clock());
        }
        assert_eq!(staged.host.state.accepted.len(), 1);
        assert!(
            staged
                .host
                .events()
                .iter()
                .any(|record| matches!(record.event, EventKind::Arrived { .. }))
        );
    }

    #[test]
    fn a_late_receiver_rejection_preserves_rows_clock_counters_and_events() {
        let mut staged = Simulation::kernel_fixture(702, 15, Execution::default()).unwrap();
        let before = staged.host.snapshot().unwrap();
        let clock = staged.host.clock();
        let mut prepared = staged.prepare_world_step(SimTime(1)).unwrap();
        prepared
            .lifecycle
            .push(Due::Birth(PregnancyId(u64::MAX - 1)));
        assert_eq!(
            staged.commit_prepared_step(prepared),
            Err(Error::Unavailable)
        );
        assert_eq!(staged.host.snapshot().unwrap(), before);
        assert_eq!(staged.host.clock(), clock);

        // The same rejected closing effect formerly followed publication.
        let mut sequential = Simulation::kernel_fixture(702, 15, Execution::default()).unwrap();
        let mut prepared = sequential.prepare_world_step(SimTime(1)).unwrap();
        prepared
            .lifecycle
            .push(Due::Birth(PregnancyId(u64::MAX - 1)));
        assert_eq!(
            sequential.commit_prepared_step_inner(prepared),
            Err(Error::Unavailable)
        );
        assert_ne!(sequential.host.snapshot().unwrap(), before);
    }

    #[test]
    fn opening_freezes_unmodified_rows_and_allocation_cursors() {
        for change_row in [false, true] {
            let mut sim = Simulation::kernel_fixture(703, 15, Execution::default()).unwrap();
            let prepared = sim.prepare_world_step(SimTime(1)).unwrap();
            if change_row {
                let inventory = sim.person_at(3).inventory;
                let stock = game_cargo::inventory_mut(&mut sim.host.storage, inventory).unwrap();
                stock.capacity = Quantity(stock.capacity.0 + 1);
            } else {
                sim.host.allocate_id().unwrap();
            }
            let before = sim.host.snapshot().unwrap();
            let clock = sim.host.clock();
            assert!(sim.commit_prepared_step(prepared).is_err());
            assert_eq!(sim.host.snapshot().unwrap(), before);
            assert_eq!(sim.host.clock(), clock);
        }
    }

    #[test]
    fn zero_elapsed_intake_matches_ordered_owner_execution() {
        let mut staged = moving_kernel();
        let mut sequential = moving_kernel();
        let now = staged.time();
        let prepared = staged.prepare_world_step(now).unwrap();
        staged.commit_prepared_step(prepared).unwrap();
        let prepared = sequential.prepare_world_step(now).unwrap();
        sequential.commit_prepared_step_inner(prepared).unwrap();
        assert_eq!(
            staged.canonical_snapshot().unwrap(),
            sequential.canonical_snapshot().unwrap()
        );
        assert_eq!(staged.host.clock(), sequential.host.clock());
    }

    #[test]
    fn changed_runtime_content_rejects_prepared_interval_without_publication() {
        let mut sim = moving_kernel();
        let prepared = sim.prepare_world_step(SimTime(1)).unwrap();
        sim.host
            .state
            .workshops
            .catalog
            .recipes
            .values_mut()
            .next()
            .unwrap()
            .duration
            .0 += 1;
        let before = sim.host.snapshot().unwrap();
        let clock = sim.host.clock();
        assert_eq!(
            sim.commit_prepared_step(prepared),
            Err(Error::InvalidSave("stale world interval inputs".into()))
        );
        assert_eq!(sim.host.snapshot().unwrap(), before);
        assert_eq!(sim.host.clock(), clock);
    }

    #[test]
    fn consumed_history_is_not_copied_but_old_arrival_evidence_survives() {
        let mut sim = moving_kernel();
        let departure = sim
            .host
            .events()
            .iter()
            .find(|record| matches!(record.event, EventKind::Departed { .. }))
            .unwrap()
            .clone();
        let person = sim.person_at(0).clone();
        sim.host
            .publish_many(vec![
                EventKind::TransferFailed {
                    actor: person.id,
                    source: person.inventory,
                };
                1_000
            ])
            .unwrap();
        sim.advance(SimDuration(1)).unwrap();
        let arrival = match departure.event {
            EventKind::Departed { arrives, .. } => arrives,
            _ => unreachable!(),
        };
        let prepared = sim.prepare_world_step(arrival).unwrap();
        let tail = sim.interval_event_tail(&prepared);
        assert!(tail.iter().any(|record| record == &departure));
        assert!(
            tail.iter()
                .all(|record| !matches!(record.event, EventKind::TransferFailed { .. }))
        );
    }
}
