//! Physical toolkits have limited concurrent users. Permission changes govern
//! new use; a working person's equipment disappears only through physical change.
use crate::*;

impl Simulation {
    fn tool_users(&self) -> BTreeMap<InventoryId, Vec<PersonId>> {
        let Some(physical) = &self.state.physical else {
            return BTreeMap::new();
        };
        let mut groups: BTreeMap<InventoryId, Vec<PersonId>> = BTreeMap::new();
        for (actor, body) in &physical.actors {
            if !body.vitals.alive || body.vitals.health == 0 {
                continue;
            }
            let selected = match body.activity {
                Activity::Work { site } => body.job.as_ref().and_then(|job| {
                    physical
                        .sites
                        .get(&site)
                        .map(|site| job.tool.unwrap_or(site.tool))
                }),
                Activity::Cultivate { field, .. } => self
                    .state
                    .environment
                    .as_ref()
                    .and_then(|world| world.fields.get(&field))
                    .map(|field| field.tool),
                Activity::Construct { project, .. } => self
                    .state
                    .projects
                    .as_ref()
                    .and_then(|world| world.projects.get(&project))
                    .map(|plan| plan.tool),
                Activity::WritingMemory { .. } => self.invocation_selected_tool(*actor),
                _ => None,
            };
            if let Some(tool) = selected
                && let Some(stock) = self.state.inventories.get(&tool)
                && stock.good == Good::Tool
                && let Some(area) = self.inventory_area(stock)
                && self.state.people[self.person_index[actor]].location == Location::AtArea(area)
            {
                groups.entry(tool).or_default().push(*actor);
            }
        }
        for (tool, users) in &mut groups {
            let cursor = self.state.inventories[tool].last_granted;
            users.sort_by_key(|actor| (cursor.is_some_and(|last| *actor <= last), *actor));
        }
        groups
    }

    /// Compute once for an interval or closing commit, before any workers finish.
    /// Released tools cannot fund another worker's already elapsed interval.
    pub(crate) fn equipment_availability(&self) -> BTreeMap<PersonId, bool> {
        let Some(physical) = &self.state.physical else {
            return BTreeMap::new();
        };
        let mut available: BTreeMap<_, _> = physical
            .actors
            .iter()
            .filter(|(_, body)| {
                matches!(
                    body.activity,
                    Activity::Work { .. }
                        | Activity::Cultivate { .. }
                        | Activity::Construct { .. }
                        | Activity::WritingMemory { .. }
                )
            })
            .map(|(actor, _)| (*actor, false))
            .collect();
        for (tool, users) in self.tool_users() {
            let slots = usize::try_from(self.state.inventories[&tool].quantity.0 / 1_000)
                .unwrap_or(usize::MAX);
            for actor in users.into_iter().take(slots) {
                available.insert(actor, true);
            }
        }
        available
    }

    /// Check a current allocation, or whether a prospective actor can take one
    /// free allocation without displacing workers already using the toolkit.
    pub(crate) fn tool_available_for(&self, actor: PersonId, tool: InventoryId) -> bool {
        let Some(index) = self.person_index.get(&actor) else {
            return false;
        };
        let Some(stock) = self.state.inventories.get(&tool) else {
            return false;
        };
        if !self
            .state
            .physical
            .as_ref()
            .and_then(|world| world.actors.get(&actor))
            .is_some_and(|body| body.vitals.alive && body.vitals.health > 0)
        {
            return false;
        }
        let Location::AtArea(area) = self.state.people[*index].location else {
            return false;
        };
        if stock.good != Good::Tool
            || stock.quantity.0 < 1_000
            || self.inventory_area(stock) != Some(area)
        {
            return false;
        }
        let users = self.tool_users().remove(&tool).unwrap_or_default();
        let slots = stock.quantity.0 / 1_000;
        if let Some(position) = users.iter().position(|user| *user == actor) {
            (position as u64) < slots
        } else {
            (users.len() as u64) < slots
        }
    }

    pub(crate) fn select_work_tool(&self, actor: PersonId, site: SiteId) -> Option<InventoryId> {
        let physical = self.state.physical.as_ref()?;
        let body = physical.actors.get(&actor)?;
        if !body.vitals.alive || body.vitals.health == 0 {
            return None;
        }
        let site = physical.sites.get(&site)?;
        let Location::AtArea(area) = self.state.people[*self.person_index.get(&actor)?].location
        else {
            return None;
        };
        if area != site.area {
            return None;
        }
        let resumed = body.job.as_ref().filter(|job| job.site == site.id);
        let users = self.tool_users();
        let mut candidates = Vec::new();
        if let Some(tool) = resumed.and_then(|job| job.tool) {
            candidates.push(tool);
        }
        candidates.push(site.tool);
        candidates.extend(
            self.inventory_index
                .get(&InventoryLocation::Carried(actor))
                .into_iter()
                .flatten()
                .copied(),
        );
        candidates.extend(
            self.inventory_index
                .get(&InventoryLocation::Area(area))
                .into_iter()
                .flatten()
                .copied(),
        );
        let mut visited = BTreeSet::new();
        candidates.into_iter().find(|tool| {
            if !visited.insert(*tool) {
                return false;
            }
            let Some(stock) = self.state.inventories.get(tool) else {
                return false;
            };
            if stock.good != Good::Tool
                || !self.has_stock_access(actor, stock)
                || self.inventory_area(stock) != Some(area)
            {
                return false;
            }
            let consumed = if resumed.is_none() && *tool == site.input {
                site.input_quantity.0
            } else {
                0
            };
            let Some(after_input) = stock.quantity.0.checked_sub(consumed) else {
                return false;
            };
            let currently_allocated = users
                .get(tool)
                .into_iter()
                .flatten()
                .take(usize::try_from(stock.quantity.0 / 1_000).unwrap_or(usize::MAX))
                .filter(|user| **user != actor)
                .count() as u64;
            after_input / 1_000 > currently_allocated
        })
    }

    #[cfg(test)]
    pub(crate) fn work_equipment_available(&self, actor: PersonId) -> bool {
        let Some(physical) = &self.state.physical else {
            return false;
        };
        let Some(body) = physical.actors.get(&actor) else {
            return false;
        };
        let Activity::Work { site } = body.activity else {
            return false;
        };
        let Some(job) = &body.job else { return false };
        let Some(site) = physical.sites.get(&site) else {
            return false;
        };
        self.tool_available_for(actor, job.tool.unwrap_or(site.tool))
    }

    pub(crate) fn cultivation_equipment_available(&self, actor: PersonId) -> bool {
        let Some(physical) = &self.state.physical else {
            return false;
        };
        let Some(body) = physical.actors.get(&actor) else {
            return false;
        };
        let Activity::Cultivate { field, .. } = body.activity else {
            return false;
        };
        self.state
            .environment
            .as_ref()
            .and_then(|world| world.fields.get(&field))
            .is_some_and(|field| self.tool_available_for(actor, field.tool))
    }

    /// Allocate an empty compartment only after every acceptance check. Loading
    /// moves the actual goods later through the existing conserved transfer path.
    pub(crate) fn carry_good(
        &mut self,
        actor: PersonId,
        source: InventoryId,
        quantity: Quantity,
    ) -> Result<(), Error> {
        self.preflight_agreements()?;
        self.preflight_institutions()?;
        let physical = self.state.physical.as_ref().ok_or(Error::Unavailable)?;
        let body = physical.actors.get(&actor).ok_or(Error::Unavailable)?;
        let person =
            &self.state.people[*self.person_index.get(&actor).ok_or(Error::Unavailable)?];
        let Location::AtArea(area) = person.location else {
            return Err(Error::Unavailable);
        };
        let stock = self
            .state
            .inventories
            .get(&source)
            .ok_or(Error::Unavailable)?;
        if quantity.0 == 0
            || !body.vitals.alive
            || body.vitals.health == 0
            || !self.independent(actor)
            || self.in_combat(actor)
            || self.combat_restricts(actor)
            || self.care_index.contains_key(&actor)
            || body.vitals.fatigue >= physical.rules.fatigue_limit
            || stock.location == InventoryLocation::Carried(actor)
            || !self.has_stock_access(actor, stock)
            || self.inventory_area(stock) != Some(area)
            || self.unreserved_stock(source, actor) < quantity.0
        {
            return Err(Error::Unavailable);
        }
        let carried: u128 = self
            .inventory_index
            .get(&InventoryLocation::Carried(actor))
            .into_iter()
            .flatten()
            .map(|id| u128::from(self.state.inventories[id].quantity.0))
            .sum();
        if carried + u128::from(quantity.0) > u128::from(physical.rules.carrying_capacity.0) {
            return Err(Error::Unavailable);
        }
        self.state
            .time
            .0
            .checked_add(physical.rules.loading_time.0)
            .ok_or(Error::Overflow)?;
        self.ensure_event_capacity(2)?;
        let good = stock.good;
        let capacity = physical.rules.carrying_capacity;
        let existing = self
            .inventory_index
            .get(&InventoryLocation::Carried(actor))
            .into_iter()
            .flatten()
            .copied()
            .find(|id| {
                let destination = &self.state.inventories[id];
                destination.good == good
                    && self.has_stock_access(actor, destination)
                    && destination.capacity.0 - destination.quantity.0 >= quantity.0
            });
        let original_next_id = self.state.next_id;
        let destination = if let Some(existing) = existing {
            existing
        } else {
            self.state.next_id.checked_add(1).ok_or(Error::Overflow)?;
            self.add_inventory(
                good,
                InventoryLocation::Carried(actor),
                Quantity(0),
                capacity,
                BTreeSet::from([actor]),
            )?
        };
        match self.debug_physical_command(
            actor,
            Command::Load {
                source,
                destination,
                quantity,
            },
        ) {
            Ok(()) => {
                self.update_property_events();
                Ok(())
            }
            Err(error) => {
                // Load validates before changing actions or goods. This fallback
                // preserves that atomic contract if another prerequisite changes.
                if existing.is_none() {
                    self.state.inventories.remove(&destination);
                    self.inventory_index
                        .get_mut(&InventoryLocation::Carried(actor))
                        .unwrap()
                        .retain(|id| *id != destination);
                    self.state.next_id = original_next_id;
                }
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sim() -> Simulation {
        Simulation::survival_fixture(1, Execution::default()).unwrap()
    }

    fn local_site(sim: &Simulation) -> SiteId {
        sim.state
            .physical
            .as_ref()
            .unwrap()
            .sites
            .values()
            .find(|site| site.area == AreaId(1))
            .unwrap()
            .id
    }

    fn duplicate_site(sim: &mut Simulation, original: SiteId) -> SiteId {
        let mut site = sim.state.physical.as_ref().unwrap().sites[&original].clone();
        let id = SiteId(allocate(&mut sim.state).unwrap());
        site.id = id;
        sim.state.physical.as_mut().unwrap().sites.insert(id, site);
        id
    }

    #[test]
    fn one_physical_toolkit_cannot_run_two_sites_and_an_owned_alternate_can() {
        let mut sim = sim();
        let first = sim.state.ruler;
        let second = sim.state.people[3].id;
        let site = local_site(&sim);
        let other = duplicate_site(&mut sim, site);
        sim.debug_physical_command(first, Command::Work { site })
            .unwrap();
        let before = sim.state.clone();
        assert!(
            sim.debug_physical_command(second, Command::Work { site: other })
                .is_err()
        );
        assert_eq!(sim.state, before);
        let alternate = sim
            .debug_add_inventory(
                Good::Tool,
                InventoryLocation::Carried(second),
                Quantity(1_000),
                Quantity(1_000),
                BTreeSet::from([second]),
            )
            .unwrap();
        assert_eq!(sim.select_work_tool(second, other), Some(alternate));
        sim.debug_physical_command(second, Command::Work { site: other })
            .unwrap();
        assert!(sim.work_equipment_available(first));
        assert!(sim.work_equipment_available(second));
        sim.advance(MINUTE).unwrap();
        assert_eq!(
            sim.state
                .events
                .iter()
                .filter(|event| matches!(event.kind,
            EventKind::Produced { actor, .. } if actor == first || actor == second))
                .count(),
            2
        );
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }

    #[test]
    fn one_closing_toolkit_cannot_be_reused_for_two_already_elapsed_shifts() {
        let mut sim = sim();
        let first = sim.state.ruler;
        let second = sim.state.people[3].id;
        let carrier = sim.state.people[4].id;
        let site = local_site(&sim);
        let other = duplicate_site(&mut sim, site);
        let tool = sim.state.physical.as_ref().unwrap().sites[&site].tool;
        sim.state.inventories.get_mut(&tool).unwrap().quantity = Quantity(2_000);
        sim.debug_physical_command(first, Command::Work { site })
            .unwrap();
        sim.debug_physical_command(second, Command::Work { site: other })
            .unwrap();
        sim.carry_good(carrier, tool, Quantity(1_000)).unwrap();
        sim.advance(MINUTE).unwrap();
        assert_eq!(
            sim.state
                .events
                .iter()
                .filter(|event| matches!(event.kind,
            EventKind::Produced { actor, .. } if actor == first || actor == second))
                .count(),
            1
        );
        let second_body = &sim.state.physical.as_ref().unwrap().actors[&second];
        assert_eq!(second_body.activity, Activity::Idle);
        assert_eq!(second_body.job.as_ref().unwrap().remaining, 0);
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }

    #[test]
    fn cultivation_and_work_cannot_allocate_the_same_toolkit() {
        let mut sim = Simulation::farming_fixture(1, Execution::default()).unwrap();
        sim.state.environment.as_mut().unwrap().crop.prepare_labor = MINUTE;
        let farmer = sim.state.ruler;
        let worker = sim.state.people[3].id;
        let field = sim
            .state
            .environment
            .as_ref()
            .unwrap()
            .fields
            .values()
            .next()
            .unwrap()
            .clone();
        sim.debug_set_personality(
            worker,
            decisions::Personality::restful(),
            decisions::Dream::Comfort,
        )
        .unwrap();
        for actor in [farmer, worker] {
            if sim.state.people[sim.person_index[&actor]].location != Location::AtArea(field.area) {
                sim.debug_move_actor(actor, field.area).unwrap();
            }
        }
        sim.advance(SimDuration(120_000)).unwrap();
        sim.debug_physical_command(worker, Command::Cancel).unwrap();
        let land = sim
            .state
            .environment
            .as_mut()
            .unwrap()
            .fields
            .get_mut(&field.id)
            .unwrap();
        land.preparation_ms = 0;
        land.phase = environment::CropPhase::Fallow;
        let input = sim
            .debug_add_inventory(
                Good::Timber,
                InventoryLocation::Area(field.area),
                Quantity(2_000),
                Quantity(2_000),
                BTreeSet::from([farmer, worker]),
            )
            .unwrap();
        let output = sim
            .debug_add_inventory(
                Good::Timber,
                InventoryLocation::Area(field.area),
                Quantity(0),
                Quantity(2_000),
                BTreeSet::from([farmer, worker]),
            )
            .unwrap();
        let site = SiteId(allocate(&mut sim.state).unwrap());
        sim.state.physical.as_mut().unwrap().sites.insert(
            site,
            physical::Site {
                id: site,
                area: field.area,
                input,
                input_quantity: Quantity(1_000),
                tool: field.tool,
                output,
                output_quantity: Quantity(1_000),
                duration: MINUTE,
                access: BTreeSet::from([farmer, worker]),
                last_granted: None,
            },
        );
        sim.debug_physical_command(
            farmer,
            Command::Cultivate {
                field: field.id,
                operation: FarmOperation::Prepare,
            },
        )
        .unwrap();
        assert!(sim.cultivation_equipment_available(farmer));
        assert!(!sim.tool_available_for(worker, field.tool));
        assert_eq!(sim.select_work_tool(worker, site), None);
        let before = sim.state.clone();
        assert!(
            sim.debug_physical_command(worker, Command::Work { site })
                .is_err()
        );
        assert_eq!(sim.state, before);
        // Two toolkits initially support both workers. One leaves precisely at
        // completion; finishing cultivation cannot reassign its closing slot to
        // the other worker's elapsed shift.
        sim.state.inventories.get_mut(&field.tool).unwrap().quantity = Quantity(2_000);
        sim.debug_physical_command(worker, Command::Work { site })
            .unwrap();
        let cargo = sim
            .debug_add_inventory(
                Good::Tool,
                InventoryLocation::Carried(worker),
                Quantity(0),
                Quantity(1_000),
                BTreeSet::from([worker]),
            )
            .unwrap();
        sim.debug_schedule_transfers(
            SimTime(sim.time().0 + MINUTE.0),
            vec![TransferIntent {
                actor: worker,
                source: field.tool,
                destination: cargo,
                quantity: Quantity(1_000),
            }],
        )
        .unwrap();
        sim.advance(MINUTE).unwrap();
        assert_eq!(
            sim.state.environment.as_ref().unwrap().fields[&field.id].phase,
            environment::CropPhase::Prepared
        );
        assert_eq!(sim.state.inventories[&output].quantity, Quantity(0));
        assert_eq!(
            sim.state.physical.as_ref().unwrap().actors[&worker]
                .job
                .as_ref()
                .unwrap()
                .remaining,
            0
        );
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }

    #[test]
    fn toolkit_removal_interrupts_but_preserves_work_for_a_real_replacement() {
        let mut sim = sim();
        sim.state.physical.as_mut().unwrap().rules.loading_time = SimDuration(30_000);
        let worker = sim.state.ruler;
        let carrier = sim.state.people[3].id;
        let site = local_site(&sim);
        let recipe = sim.state.physical.as_ref().unwrap().sites[&site].clone();
        let alternate = sim
            .debug_add_inventory(
                Good::Tool,
                InventoryLocation::Carried(worker),
                Quantity(1_000),
                Quantity(1_000),
                BTreeSet::from([worker]),
            )
            .unwrap();
        sim.debug_physical_command(worker, Command::Work { site })
            .unwrap();
        assert_eq!(
            sim.state.physical.as_ref().unwrap().actors[&worker]
                .job
                .as_ref()
                .unwrap()
                .tool,
            Some(recipe.tool)
        );
        let input_after_start = sim.state.inventories[&recipe.input].quantity;
        sim.carry_good(carrier, recipe.tool, Quantity(1_000))
            .unwrap();
        sim.advance(SimDuration(30_000)).unwrap();
        let body = &sim.state.physical.as_ref().unwrap().actors[&worker];
        assert_eq!(body.activity, Activity::Idle);
        assert_eq!(body.job.as_ref().unwrap().remaining, 30_000);
        assert_eq!(sim.state.inventories[&recipe.tool].quantity, Quantity(0));
        sim.debug_physical_command(worker, Command::Work { site })
            .unwrap();
        assert_eq!(
            sim.state.physical.as_ref().unwrap().actors[&worker]
                .job
                .as_ref()
                .unwrap()
                .tool,
            Some(alternate)
        );
        assert_eq!(
            sim.state.inventories[&recipe.input].quantity,
            input_after_start
        );
        sim.advance(SimDuration(30_000)).unwrap();
        assert!(
            sim.state
                .events
                .iter()
                .any(|event| event.time == SimTime(60_000)
                    && matches!(event.kind, EventKind::Produced { actor, .. } if actor == worker))
        );
        assert_eq!(
            sim.state.inventories[&recipe.input].quantity,
            input_after_start
        );
    }

    #[test]
    fn closing_transfer_prevents_output_without_erasing_the_elapsed_opening_work() {
        let mut sim = sim();
        let worker = sim.state.ruler;
        let carrier = sim.state.people[3].id;
        let site = local_site(&sim);
        let tool = sim.state.physical.as_ref().unwrap().sites[&site].tool;
        sim.debug_physical_command(worker, Command::Work { site })
            .unwrap();
        sim.carry_good(carrier, tool, Quantity(1_000)).unwrap();
        sim.advance(MINUTE).unwrap();
        let body = &sim.state.physical.as_ref().unwrap().actors[&worker];
        assert_eq!(body.activity, Activity::Idle);
        assert_eq!(body.job.as_ref().unwrap().remaining, 0);
        assert!(!sim.state.events.iter().any(
            |event| matches!(event.kind, EventKind::Produced { actor, .. } if actor == worker)
        ));
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }

    #[test]
    fn an_access_withdrawal_does_not_physically_remove_a_tool_already_in_use() {
        let mut sim = sim();
        let actor = sim.state.ruler;
        let site = local_site(&sim);
        let tool = sim.state.physical.as_ref().unwrap().sites[&site].tool;
        sim.debug_physical_command(actor, Command::Work { site })
            .unwrap();
        sim.state
            .inventories
            .get_mut(&tool)
            .unwrap()
            .access
            .remove(&actor);
        assert!(sim.work_equipment_available(actor));
        sim.advance(MINUTE).unwrap();
        assert!(sim.state.events.iter().any(|event| matches!(event.kind, EventKind::Produced { actor: worker, .. } if worker == actor)));
        let before = sim.state.clone();
        assert!(
            sim.debug_physical_command(actor, Command::Work { site })
                .is_err()
        );
        assert_eq!(sim.state, before);
    }

    #[test]
    fn generic_carry_has_atomic_validation_and_one_shared_capacity_for_all_goods() {
        let mut sim = sim();
        let actor = sim.state.ruler;
        let tools = sim.state.physical.as_ref().unwrap().sites[&local_site(&sim)].tool;
        let opening = sim.state.clone();
        for (source, quantity) in [
            (InventoryId(u64::MAX), Quantity(1_000)),
            (tools, Quantity(0)),
            (tools, Quantity(2_000)),
        ] {
            assert!(sim.carry_good(actor, source, quantity).is_err());
            assert_eq!(sim.state, opening);
        }
        sim.carry_good(actor, tools, Quantity(1_000)).unwrap();
        let destination = match sim.state.physical.as_ref().unwrap().actors[&actor].activity {
            Activity::Load { destination, .. } => destination,
            _ => panic!("carry must schedule loading"),
        };
        assert_eq!(sim.state.inventories[&destination].quantity, Quantity(0));
        sim.advance(MINUTE).unwrap();
        assert_eq!(
            sim.state.inventories[&destination].quantity,
            Quantity(1_000)
        );
        let water = sim
            .debug_add_inventory(
                Good::Water,
                InventoryLocation::Area(AreaId(1)),
                Quantity(10_000),
                Quantity(10_000),
                BTreeSet::from([actor]),
            )
            .unwrap();
        sim.carry_good(actor, water, Quantity(9_000)).unwrap();
        sim.advance(MINUTE).unwrap();
        let full = sim.state.clone();
        let seed = sim
            .debug_add_inventory(
                Good::Seed,
                InventoryLocation::Area(AreaId(1)),
                Quantity(1_000),
                Quantity(1_000),
                BTreeSet::from([actor]),
            )
            .unwrap();
        let before = sim.state.clone();
        assert!(sim.carry_good(actor, seed, Quantity(1_000)).is_err());
        assert_eq!(sim.state, before);
        assert!(sim.state.inventories.len() > full.inventories.len());
        Simulation::load(
            &sim.save().unwrap(),
            Execution {
                workers: 4,
                reverse_jobs: true,
            },
        )
        .unwrap();
    }

    #[test]
    fn carry_preserves_provenance_and_pending_empty_compartments_through_saves() {
        let mut sim = Simulation::property_fixture(1, Execution::default()).unwrap();
        let actor = sim.state.ruler;
        let source = sim
            .state
            .inventories
            .values()
            .find(|stock| {
                stock.good == Good::Tool
                    && stock.location == InventoryLocation::Area(AreaId(1))
                    && sim.has_stock_access(actor, stock)
            })
            .unwrap()
            .id;
        sim.carry_good(actor, source, Quantity(1_000)).unwrap();
        let mut restored = Simulation::load(
            &sim.save().unwrap(),
            Execution {
                workers: 4,
                reverse_jobs: true,
            },
        )
        .unwrap();
        sim.advance(MINUTE).unwrap();
        restored.advance(SimDuration(1)).unwrap();
        restored.advance(SimDuration(59_999)).unwrap();
        assert_eq!(sim.state, restored.state);
        let destination = sim
            .state
            .inventories
            .values()
            .find(|stock| {
                stock.good == Good::Tool && stock.location == InventoryLocation::Carried(actor)
            })
            .unwrap()
            .id;
        assert_eq!(
            sim.state.inventories[&destination].quantity,
            Quantity(1_000)
        );
        assert_eq!(
            sim.state.property.as_ref().unwrap().portions[&destination]
                .values()
                .sum::<u64>(),
            1_000
        );
    }
}
