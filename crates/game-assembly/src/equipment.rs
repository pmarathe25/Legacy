//! Physical toolkits have limited concurrent users. Permission changes govern
//! new use; a working person's equipment disappears only through physical change.
use crate::*;

pub(crate) struct RankingResources {
    pub(crate) reservations: game_cargo::ReservationTotals,
    allocations: game_cargo::ToolAllocations,
}

impl Simulation {
    /// Bind a contact handoff to an actual compartment and the normal timed
    /// transfer action. Permission still comes from the receiving owner.
    pub(crate) fn start_contact_delivery(
        &mut self,
        actor: PersonId,
        source: InventoryId,
        recipient: PersonId,
        quantity: Quantity,
    ) -> Result<(), Error> {
        self.preflight_agreements()?;
        self.preflight_institutions()?;
        let plan = self.prepare_carried_compartment(actor, source, recipient, quantity, true)?;
        self.apply_carried_compartment(plan)
    }

    pub(crate) fn prepare_carried_compartment(
        &self,
        actor: PersonId,
        source: InventoryId,
        recipient: PersonId,
        quantity: Quantity,
        handoff: bool,
    ) -> Result<game_cargo::PreparedCarry, Error> {
        let actor_facts = self.cargo_actor_facts(actor)?;
        let stock = game_cargo::inventory(&self.host.storage, source).ok_or(Error::Unavailable)?;
        let recipient_alive = self
            .host
            .state
            .physical
            .as_ref()
            .and_then(|_| game_physiology::body(&self.host.storage, recipient))
            .is_some_and(|body| body.vitals.alive);
        if !self.person_index.contains_key(&recipient) {
            return Err(Error::Unavailable);
        }
        let capacity = self.body_rules(recipient).carrying_capacity;
        let proposed = Inventory {
            id: InventoryId(self.host.next_id()),
            location: InventoryLocation::Carried(recipient),
            good: stock.good,
            quantity: Quantity(0),
            capacity,
            access: BTreeSet::from([recipient]),
            last_granted: None,
            spoils_at: None,
        };
        let candidates: Vec<_> = self
            .inventory_index
            .get(&InventoryLocation::Carried(recipient))
            .into_iter()
            .flatten()
            .map(|id| game_cargo::Compartment {
                stock: game_cargo::inventory(&self.host.storage, *(id)).expect("inventory exists"),
                receiver: self.cargo_receiver_facts(
                    actor,
                    source,
                    game_cargo::inventory(&self.host.storage, *(id)).expect("inventory exists"),
                    quantity,
                ),
            })
            .collect();
        let allocation = game_cargo::NewCompartment {
            id: proposed.id,
            holder: recipient,
            alive: recipient_alive,
            capacity,
            receiver: self.cargo_receiver_facts(actor, source, &proposed, quantity),
        };
        let source_facts = self.cargo_source_facts(actor, stock, quantity);
        if handoff {
            game_cargo::prepare_handoff(
                actor_facts,
                stock,
                quantity,
                source_facts,
                &candidates,
                allocation,
                self.host.clock().time,
                self.body_rules(actor).loading_time,
            )
        } else {
            game_cargo::prepare_carry(
                actor_facts,
                stock,
                quantity,
                source_facts,
                &candidates,
                allocation,
                self.host.clock().time,
                self.body_rules(actor).loading_time,
            )
        }
    }
    /// All native-load facts and global allocation/event capacity are validated
    /// before allocating the sole actual compartment or interrupting its actor.
    fn apply_carried_compartment(&mut self, plan: game_cargo::PreparedCarry) -> Result<(), Error> {
        self.ensure_event_capacity(3)?;
        if let game_cargo::Selection::Proposed(stock) = &plan.selection {
            self.host.next_id().checked_add(1).ok_or(Error::Overflow)?;
            let id = self.add_inventory(
                stock.good,
                stock.location.clone(),
                stock.quantity,
                stock.capacity,
                stock.access.clone(),
            )?;
            debug_assert_eq!(id, stock.id);
        }
        self.apply_cargo_load(plan.job)
    }
    pub(crate) fn prepare_ranking_resources(&self) -> Result<RankingResources, Error> {
        Ok(RankingResources {
            reservations: self.reservation_totals(None)?,
            allocations: self.prepare_tool_allocations(None)?,
        })
    }

    fn prepare_tool_allocations(
        &self,
        only: Option<InventoryId>,
    ) -> Result<game_cargo::ToolAllocations, Error> {
        self.host
            .state
            .physical
            .as_ref()
            .ok_or(Error::Unavailable)?;
        let mut uses = Vec::new();
        for (actor, body) in game_physiology::bodies(&self.host.storage) {
            let actor = &actor;
            let activity = self.host.storage.activity(*actor).expect("activity exists");
            if !body.vitals.alive || body.vitals.health == 0 {
                continue;
            }
            let selected = match *activity {
                Activity::Work { site } => {
                    game_workshop::job(&self.host.storage, *actor).and_then(|job| {
                        game_workshop::site(&self.host.storage, site)
                            .map(|site| job.tool.unwrap_or(site.tool))
                    })
                }
                Activity::Cultivate { field, .. } => self
                    .host
                    .state
                    .agriculture
                    .as_ref()
                    .and_then(|world| world.field(&self.host.storage, field))
                    .map(|field| field.tool),
                Activity::Instruction { .. } | Activity::Practice { .. } => {
                    self.social_selected_tool(*actor)
                }
                Activity::HealthWork { .. } => self.health_selected_tool(*actor),
                Activity::Gather { .. } => self.ecology_job_tool(*actor),
                Activity::Construct { project, .. } => self
                    .host
                    .state
                    .projects
                    .as_ref()
                    .and_then(|world| world.projects.get(&project))
                    .map(|plan| plan.tool),
                Activity::WritingMemory { .. } => self.invocation_selected_tool(*actor),
                _ => None,
            };
            if let Some(tool) = selected
                && only.is_none_or(|wanted| tool == wanted)
                && let Some(stock) = game_cargo::inventory(&self.host.storage, tool)
                && stock.good == Good::Tool
                && let Some(area) = self.inventory_area(stock)
                && *game_spatial::location(
                    &self.host.storage,
                    game_ecs::StableId((self.host.state.people[self.person_index[actor]].id).0),
                )
                .expect("location exists")
                    == Location::AtArea(area)
            {
                uses.push(game_cargo::ToolUse {
                    actor: *actor,
                    tool,
                });
            }
        }
        let selected = |stock: &Inventory| {
            (stock.good == Good::Tool).then_some(game_cargo::ToolSlots {
                tool: stock.id,
                quantity: stock.quantity,
                last_granted: stock.last_granted,
            })
        };
        let tools = match only {
            Some(id) => game_cargo::inventory(&self.host.storage, id)
                .and_then(selected)
                .into_iter()
                .collect::<Vec<_>>(),
            None => game_cargo::inventories(&self.host.storage)
                .map(|(_, row)| row)
                .filter_map(selected)
                .collect(),
        };
        game_cargo::ToolAllocations::prepare(tools, uses)
    }

    /// Compute once for an interval or closing commit, before any workers finish.
    /// Released tools cannot fund another worker's already elapsed interval.
    pub(crate) fn equipment_availability(&self) -> BTreeMap<PersonId, bool> {
        let Some(_) = &self.host.state.physical else {
            return BTreeMap::new();
        };
        let mut available: BTreeMap<_, _> = self
            .host
            .storage
            .activities()
            .filter(|(_, activity)| {
                matches!(
                    **activity,
                    Activity::Work { .. }
                        | Activity::Instruction { .. }
                        | Activity::Practice { .. }
                        | Activity::HealthWork { .. }
                        | Activity::Gather { .. }
                        | Activity::Cultivate { .. }
                        | Activity::Construct { .. }
                        | Activity::WritingMemory { .. }
                )
            })
            .map(|(actor, activity)| {
                (
                    actor,
                    matches!(*activity, Activity::Gather { .. })
                        && self.ecology_job_tool(actor).is_none(),
                )
            })
            .collect();
        if let Ok(allocations) = self.prepare_tool_allocations(None) {
            for actor in allocations.allocated_actors() {
                available.insert(actor, true);
            }
        }
        available
    }

    /// Check a current allocation, or whether a prospective actor can take one
    /// free allocation without displacing workers already using the toolkit.
    pub(crate) fn tool_available_for(&self, actor: PersonId, tool: InventoryId) -> bool {
        self.tool_available_for_planning(actor, tool, None)
    }

    pub(crate) fn tool_available_for_planning(
        &self,
        actor: PersonId,
        tool: InventoryId,
        resources: Option<&RankingResources>,
    ) -> bool {
        let Some(index) = self.person_index.get(&actor) else {
            return false;
        };
        let Some(stock) = game_cargo::inventory(&self.host.storage, tool) else {
            return false;
        };
        if !self
            .host
            .state
            .physical
            .as_ref()
            .and_then(|_| game_physiology::body(&self.host.storage, actor))
            .is_some_and(|body| body.vitals.alive && body.vitals.health > 0)
        {
            return false;
        }
        let Location::AtArea(area) = *game_spatial::location(
            &self.host.storage,
            game_ecs::StableId((self.host.state.people[*index].id).0),
        )
        .expect("location exists") else {
            return false;
        };
        if stock.good != Good::Tool
            || stock.quantity.0 < 1_000
            || self.inventory_area(stock) != Some(area)
        {
            return false;
        }
        if let Some(resources) = resources {
            return resources.allocations.allows(actor, tool);
        }
        self.prepare_tool_allocations(Some(tool))
            .is_ok_and(|allocations| allocations.allows(actor, tool))
    }

    pub(crate) fn select_work_tool(&self, actor: PersonId, site: SiteId) -> Option<InventoryId> {
        self.host.state.physical.as_ref()?;
        let body = game_physiology::body(&self.host.storage, actor)?;
        if !body.vitals.alive || body.vitals.health == 0 {
            return None;
        }
        let site = game_workshop::site(&self.host.storage, site)?;
        let Location::AtArea(area) = *game_spatial::location(
            &self.host.storage,
            game_ecs::StableId((self.host.state.people[*self.person_index.get(&actor)?].id).0),
        )
        .expect("location exists") else {
            return None;
        };
        if area != site.area {
            return None;
        }
        let resumed =
            game_workshop::job(&self.host.storage, actor).filter(|job| job.site == site.id);
        let allocations = self.prepare_tool_allocations(None).ok()?;
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
            let Some(stock) = game_cargo::inventory(&self.host.storage, *(tool)) else {
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
            let currently_allocated = allocations
                .allocated_users(*tool)
                .filter(|user| *user != actor)
                .count() as u64;
            after_input / 1_000 > currently_allocated
        })
    }

    #[cfg(test)]
    pub(crate) fn work_equipment_available(&self, actor: PersonId) -> bool {
        let Some(_) = &self.host.state.physical else {
            return false;
        };
        if game_physiology::body(&self.host.storage, actor).is_none() {
            return false;
        }
        let Some(Activity::Work { site }) = self.host.storage.activity(actor) else {
            return false;
        };
        let Some(job) = game_workshop::job(&self.host.storage, actor) else {
            return false;
        };
        let Some(site) = game_workshop::site(&self.host.storage, *site) else {
            return false;
        };
        self.tool_available_for(actor, job.tool.unwrap_or(site.tool))
    }

    pub(crate) fn cultivation_equipment_available(&self, actor: PersonId) -> bool {
        let Some(_) = &self.host.state.physical else {
            return false;
        };
        if game_physiology::body(&self.host.storage, actor).is_none() {
            return false;
        }
        let Some(Activity::Cultivate { field, .. }) = self.host.storage.activity(actor) else {
            return false;
        };
        self.host
            .state
            .agriculture
            .as_ref()
            .and_then(|world| world.field(&self.host.storage, *field))
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
        let plan = self.prepare_carried_compartment(actor, source, actor, quantity, false)?;
        self.apply_carried_compartment(plan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sim() -> Simulation {
        Simulation::survival_fixture(1, Execution::default()).unwrap()
    }

    fn local_site(sim: &Simulation) -> SiteId {
        game_workshop::sites(&sim.host.storage)
            .map(|(_, site)| site)
            .find(|site| site.area == AreaId(1))
            .unwrap()
            .id
    }

    fn duplicate_site(sim: &mut Simulation, original: SiteId) -> SiteId {
        let mut site = game_workshop::site(&sim.host.storage, original)
            .expect("site exists")
            .clone();
        let id = SiteId(sim.host.allocate_id().unwrap().0);
        site.id = id;
        game_workshop::insert_site(&mut sim.host.storage, site).unwrap();
        id
    }

    fn compare_resource_queries(sim: &Simulation) {
        let resources = sim.prepare_ranking_resources().unwrap();
        for person in &sim.host.state.people {
            for stock in game_cargo::inventories(&sim.host.storage).map(|(_, row)| row) {
                assert_eq!(
                    sim.unreserved_stock(stock.id, person.id),
                    sim.unreserved_stock_for_planning(stock.id, person.id, Some(&resources)),
                );
                assert_eq!(
                    sim.tool_available_for(person.id, stock.id),
                    sim.tool_available_for_planning(person.id, stock.id, Some(&resources)),
                );
            }
        }
    }

    #[test]
    fn ranking_projection_matches_live_provider_facts_without_mutation() {
        let sim = Simulation::ecology_fixture(47, Execution::default()).unwrap();
        let before = sim.canonical_snapshot().unwrap();
        let resources = sim.prepare_ranking_resources().unwrap();
        compare_resource_queries(&sim);
        for person in &sim.host.state.people {
            let actor = person.id;
            assert_eq!(
                sim.health_candidate(actor, None),
                sim.health_candidate(actor, Some(&resources))
            );
            assert_eq!(
                sim.ecology_opportunities(actor, None),
                sim.ecology_opportunities(actor, Some(&resources))
            );
            assert_eq!(
                sim.project_opportunities(actor, None),
                sim.project_opportunities(actor, Some(&resources))
            );
            assert_eq!(
                sim.autonomous_cultivation(actor, None),
                sim.autonomous_cultivation(actor, Some(&resources))
            );
            let delivery = game_logistics::Delivery::new(
                game_logistics::Requirement {
                    good: Good::Water,
                    quantity: Quantity(1_000),
                    source: None,
                },
                game_logistics::Destination::Contact(actor),
            )
            .unwrap();
            let destination = game_logistics::KnownDestination {
                destination: delivery.destination,
                location: sim.delivery_observed_location(actor),
                observed_at: sim.host.clock().time,
                accessible: true,
                room: None,
                direct_transfer: true,
            };
            assert_eq!(
                sim.plan_delivery(actor, &delivery, &destination, Quantity(1_000)),
                sim.plan_delivery_for_planning(
                    actor,
                    &delivery,
                    &destination,
                    Quantity(1_000),
                    &[],
                    Some(&resources)
                )
            );
        }
        assert_eq!(sim.canonical_snapshot().unwrap(), before);
    }

    #[test]
    fn ranking_projection_extracts_care_doses_load_amounts_and_health_water() {
        let mut sim = Simulation::health_fixture(48, Execution::default()).unwrap();
        let actors = sim
            .host
            .state
            .people
            .iter()
            .take(5)
            .map(|person| person.id)
            .collect::<Vec<_>>();
        assert_eq!(actors.len(), 5);
        for actor in sim
            .host
            .state
            .people
            .iter()
            .map(|person| person.id)
            .collect::<Vec<_>>()
        {
            sim.interrupt_actor_activity(actor);
        }
        let access = actors.iter().copied().collect();
        let water = sim
            .add_inventory(
                Good::Water,
                InventoryLocation::Area(AreaId(1)),
                Quantity(10_000),
                Quantity(10_000),
                access,
            )
            .unwrap();
        let food = sim.host.state.people[sim.person_index[&actors[0]]].inventory;
        let completes = SimTime(sim.host.clock().time.0 + MINUTE.0);
        *sim.host.storage.activity_mut(actors[0]).unwrap() = Activity::Eat {
            source: food,
            completes,
        };
        *sim.host.storage.activity_mut(actors[1]).unwrap() = Activity::Care {
            child: actors[0],
            source: water,
            completes,
        };
        *sim.host.storage.activity_mut(actors[2]).unwrap() = Activity::Load {
            source: water,
            destination: food,
            quantity: Quantity(277),
            completes,
        };
        for (actor, work) in [
            (
                actors[3],
                game_health::Work::Drink {
                    source: water,
                    quantity: Quantity(311),
                },
            ),
            (
                actors[4],
                game_health::Work::Boil {
                    source: water,
                    quantity: Quantity(509),
                    tool: food,
                },
            ),
        ] {
            game_health::insert_job(
                &mut sim.host.storage,
                game_health::Job {
                    actor,
                    area: AreaId(1),
                    completes,
                    work,
                },
            )
            .unwrap();
        }
        let care = sim.care_dose(Good::Water).unwrap().0;
        let resources = sim.prepare_ranking_resources().unwrap();
        assert_eq!(
            sim.unreserved_stock_for_planning(water, actors[0], Some(&resources)),
            10_000 - care - 277 - 311 - 509
        );
        assert_eq!(
            sim.unreserved_stock_for_planning(water, actors[1], Some(&resources)),
            10_000 - 277 - 311 - 509
        );
        assert_eq!(
            sim.unreserved_stock_for_planning(water, actors[2], Some(&resources)),
            10_000 - care - 311 - 509
        );
        assert_eq!(
            sim.unreserved_stock_for_planning(water, actors[3], Some(&resources)),
            10_000 - care - 277 - 509
        );
        assert_eq!(
            sim.unreserved_stock_for_planning(water, actors[4], Some(&resources)),
            10_000 - care - 277 - 311
        );
        assert_eq!(
            sim.unreserved_stock_for_planning(food, actors[1], Some(&resources)),
            game_cargo::inventory(&sim.host.storage, food)
                .expect("inventory exists")
                .quantity
                .0
                .saturating_sub(sim.host.state.physical.as_ref().unwrap().rules.meal_size.0)
        );
        compare_resource_queries(&sim);
    }

    #[test]
    fn ranking_projection_rebuilds_after_cancellation_death_and_tool_movement() {
        let mut sim = Simulation::autonomy_fixture(49, Execution::default()).unwrap();
        let worker = sim.host.state.ruler;
        let carrier = sim.host.state.people[3].id;
        for actor in sim
            .host
            .state
            .people
            .iter()
            .map(|person| person.id)
            .collect::<Vec<_>>()
        {
            sim.interrupt_actor_activity(actor);
        }
        let site = local_site(&sim);
        let tool = game_workshop::site(&sim.host.storage, site)
            .expect("site exists")
            .tool;
        sim.debug_actor_command(worker, Command::Work { site })
            .unwrap();
        compare_resource_queries(&sim);
        assert!(!sim.tool_available_for(carrier, tool));
        sim.debug_actor_command(worker, Command::Rest).unwrap();
        compare_resource_queries(&sim);
        assert!(sim.tool_available_for(carrier, tool));
        sim.debug_actor_command(worker, Command::Work { site })
            .unwrap();
        let opening = sim.save().unwrap();
        let mut dead = Simulation::load(&opening, Execution::default()).unwrap();
        dead.debug_injure(worker, dead.body_rules(worker).max_health)
            .unwrap();
        assert!(
            !game_physiology::body(&dead.host.storage, worker)
                .expect("body exists")
                .vitals
                .alive
        );
        compare_resource_queries(&dead);
        assert!(dead.tool_available_for(carrier, tool));
        Simulation::load(&dead.save().unwrap(), Execution::default()).unwrap();

        sim.carry_good(carrier, tool, Quantity(1_000)).unwrap();
        compare_resource_queries(&sim);
        let carried = match *sim.host.storage.activity(carrier).expect("body exists") {
            Activity::Load { destination, .. } => destination,
            _ => panic!("actual carry schedules loading"),
        };
        let save = sim.save().unwrap();
        let mut chunked = Simulation::load(
            &save,
            Execution {
                workers: 4,
                reverse_jobs: true,
            },
        )
        .unwrap();
        sim.advance(MINUTE).unwrap();
        chunked.advance(SimDuration(MINUTE.0 / 2)).unwrap();
        chunked = Simulation::load(
            &chunked.save().unwrap(),
            Execution {
                workers: 2,
                reverse_jobs: false,
            },
        )
        .unwrap();
        chunked.advance(SimDuration(MINUTE.0 / 2)).unwrap();
        assert_eq!(sim.canonical_state(), chunked.canonical_state());
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, tool)
                .expect("inventory exists")
                .quantity,
            Quantity(0)
        );
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, carried)
                .expect("inventory exists")
                .quantity,
            Quantity(1_000)
        );
        compare_resource_queries(&sim);
        sim.debug_move_actor(carrier, AreaId(2)).unwrap();
        assert!(!sim.tool_available_for(worker, carried));
        compare_resource_queries(&sim);
    }

    #[test]
    fn one_physical_toolkit_cannot_run_two_sites_and_an_owned_alternate_can() {
        let mut sim = sim();
        let first = sim.host.state.ruler;
        let second = sim.host.state.people[3].id;
        let site = local_site(&sim);
        let other = duplicate_site(&mut sim, site);
        sim.debug_actor_command(first, Command::Work { site })
            .unwrap();
        let before = sim.canonical_snapshot().unwrap();
        assert!(
            sim.debug_actor_command(second, Command::Work { site: other })
                .is_err()
        );
        assert_eq!(sim.canonical_snapshot().unwrap(), before);
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
        sim.debug_actor_command(second, Command::Work { site: other })
            .unwrap();
        assert!(sim.work_equipment_available(first));
        assert!(sim.work_equipment_available(second));
        sim.advance(MINUTE).unwrap();
        assert_eq!(
            sim.host
                .events()
                .iter()
                .filter(|event| matches!(event.event,
            EventKind::Produced { actor, .. } if actor == first || actor == second))
                .count(),
            2
        );
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }

    #[test]
    fn one_closing_toolkit_cannot_be_reused_for_two_already_elapsed_shifts() {
        let mut sim = sim();
        let first = sim.host.state.ruler;
        let second = sim.host.state.people[3].id;
        let carrier = sim.host.state.people[4].id;
        let site = local_site(&sim);
        let other = duplicate_site(&mut sim, site);
        let tool = game_workshop::site(&sim.host.storage, site)
            .expect("site exists")
            .tool;
        game_cargo::inventory_mut(&mut sim.host.storage, tool)
            .unwrap()
            .quantity = Quantity(2_000);
        sim.debug_actor_command(first, Command::Work { site })
            .unwrap();
        sim.debug_actor_command(second, Command::Work { site: other })
            .unwrap();
        sim.carry_good(carrier, tool, Quantity(1_000)).unwrap();
        sim.advance(MINUTE).unwrap();
        assert_eq!(
            sim.host
                .events()
                .iter()
                .filter(|event| matches!(event.event,
            EventKind::Produced { actor, .. } if actor == first || actor == second))
                .count(),
            1
        );
        assert_eq!(
            *sim.host.storage.activity(second).expect("activity exists"),
            Activity::Idle
        );
        assert_eq!(
            game_workshop::job(&sim.host.storage, second)
                .unwrap()
                .remaining,
            0
        );
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }

    #[test]
    fn cultivation_and_work_cannot_allocate_the_same_toolkit() {
        let mut sim = Simulation::farming_fixture(1, Execution::default()).unwrap();
        // Replace fixture work durations only after canceling its opening jobs.
        for actor in sim
            .host
            .state
            .people
            .iter()
            .map(|p| p.id)
            .collect::<Vec<_>>()
        {
            sim.interrupt_actor_activity(actor);
        }
        let agriculture = sim.host.state.agriculture.as_mut().unwrap();
        agriculture.content = None;
        agriculture
            .catalog
            .crops
            .get_mut(&agriculture.catalog.default_crop)
            .unwrap()
            .prepare_labor = MINUTE;
        let farmer = sim.host.state.ruler;
        let worker = sim.host.state.people[3].id;
        let field = sim
            .host
            .state
            .agriculture
            .as_ref()
            .unwrap()
            .fields(&sim.host.storage)
            .map(|(_, value)| value)
            .next()
            .unwrap()
            .clone();
        sim.debug_set_personality(
            worker,
            decisions::Personality::restful(),
            decisions::Aspiration::Comfort,
        )
        .unwrap();
        for actor in [farmer, worker] {
            if *game_spatial::location(
                &sim.host.storage,
                game_ecs::StableId((sim.host.state.people[sim.person_index[&actor]].id).0),
            )
            .expect("location exists")
                != Location::AtArea(field.area)
            {
                sim.debug_move_actor(actor, field.area).unwrap();
            }
        }
        sim.advance(SimDuration(120_000)).unwrap();
        sim.debug_actor_command(worker, Command::Cancel).unwrap();
        let land = sim
            .host
            .state
            .agriculture
            .as_mut()
            .unwrap()
            .crop_mut(&mut sim.host.storage, field.id)
            .unwrap();
        land.preparation_ms = 0;
        land.phase = agriculture::CropPhase::Fallow;
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
        let site = SiteId(sim.host.allocate_id().unwrap().0);
        game_workshop::insert_site(
            &mut sim.host.storage,
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
        )
        .expect("validated site insertion");
        sim.debug_actor_command(
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
        let before = sim.canonical_snapshot().unwrap();
        assert!(
            sim.debug_actor_command(worker, Command::Work { site })
                .is_err()
        );
        assert_eq!(sim.canonical_snapshot().unwrap(), before);
        // Two toolkits initially support both workers. One leaves precisely at
        // completion; finishing cultivation cannot reassign its closing slot to
        // the other worker's elapsed shift.
        let tool = game_cargo::inventory_mut(&mut sim.host.storage, field.tool).unwrap();
        tool.capacity = Quantity(2_000);
        tool.quantity = Quantity(2_000);
        sim.debug_actor_command(worker, Command::Work { site })
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
            sim.host
                .state
                .agriculture
                .as_ref()
                .unwrap()
                .crop(&sim.host.storage, field.id)
                .expect("component exists")
                .phase,
            agriculture::CropPhase::Prepared
        );
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, output)
                .expect("inventory exists")
                .quantity,
            Quantity(0)
        );
        assert_eq!(
            game_workshop::job(&sim.host.storage, worker)
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
        sim.host.state.physical.as_mut().unwrap().rules.loading_time = SimDuration(30_000);
        let worker = sim.host.state.ruler;
        let carrier = sim.host.state.people[3].id;
        let site = local_site(&sim);
        let recipe = game_workshop::site(&sim.host.storage, site)
            .expect("site exists")
            .clone();
        let alternate = sim
            .debug_add_inventory(
                Good::Tool,
                InventoryLocation::Carried(worker),
                Quantity(1_000),
                Quantity(1_000),
                BTreeSet::from([worker]),
            )
            .unwrap();
        sim.debug_actor_command(worker, Command::Work { site })
            .unwrap();
        assert_eq!(
            game_workshop::job(&sim.host.storage, worker)
                .as_ref()
                .unwrap()
                .tool,
            Some(recipe.tool)
        );
        let input_after_start = game_cargo::inventory(&sim.host.storage, recipe.input)
            .expect("inventory exists")
            .quantity;
        sim.carry_good(carrier, recipe.tool, Quantity(1_000))
            .unwrap();
        sim.advance(SimDuration(30_000)).unwrap();
        assert_eq!(
            *sim.host.storage.activity(worker).expect("activity exists"),
            Activity::Idle
        );
        assert_eq!(
            game_workshop::job(&sim.host.storage, worker)
                .unwrap()
                .remaining,
            30_000
        );
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, recipe.tool)
                .expect("inventory exists")
                .quantity,
            Quantity(0)
        );
        sim.debug_actor_command(worker, Command::Work { site })
            .unwrap();
        assert_eq!(
            game_workshop::job(&sim.host.storage, worker)
                .as_ref()
                .unwrap()
                .tool,
            Some(alternate)
        );
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, recipe.input)
                .expect("inventory exists")
                .quantity,
            input_after_start
        );
        sim.advance(SimDuration(30_000)).unwrap();
        assert!(
            sim.host
                .events()
                .iter()
                .any(|event| event.at == SimTime(60_000)
                    && matches!(event.event, EventKind::Produced { actor, .. } if actor == worker))
        );
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, recipe.input)
                .expect("inventory exists")
                .quantity,
            input_after_start
        );
    }

    #[test]
    fn closing_transfer_prevents_output_without_erasing_the_elapsed_opening_work() {
        let mut sim = sim();
        let worker = sim.host.state.ruler;
        let carrier = sim.host.state.people[3].id;
        let site = local_site(&sim);
        let tool = game_workshop::site(&sim.host.storage, site)
            .expect("site exists")
            .tool;
        sim.debug_actor_command(worker, Command::Work { site })
            .unwrap();
        sim.carry_good(carrier, tool, Quantity(1_000)).unwrap();
        sim.advance(MINUTE).unwrap();
        assert_eq!(
            *sim.host.storage.activity(worker).expect("activity exists"),
            Activity::Idle
        );
        assert_eq!(
            game_workshop::job(&sim.host.storage, worker)
                .unwrap()
                .remaining,
            0
        );
        assert!(!sim.host.events().iter().any(
            |event| matches!(event.event, EventKind::Produced { actor, .. } if actor == worker)
        ));
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }

    #[test]
    fn an_access_withdrawal_does_not_physically_remove_a_tool_already_in_use() {
        let mut sim = sim();
        let actor = sim.host.state.ruler;
        let site = local_site(&sim);
        let tool = game_workshop::site(&sim.host.storage, site)
            .expect("site exists")
            .tool;
        sim.debug_actor_command(actor, Command::Work { site })
            .unwrap();
        game_cargo::inventory_mut(&mut sim.host.storage, tool)
            .unwrap()
            .access
            .remove(&actor);
        assert!(sim.work_equipment_available(actor));
        sim.advance(MINUTE).unwrap();
        assert!(sim.host.events().iter().any(|event| matches!(event.event, EventKind::Produced { actor: worker, .. } if worker == actor)));
        let before = sim.canonical_snapshot().unwrap();
        assert!(
            sim.debug_actor_command(actor, Command::Work { site })
                .is_err()
        );
        assert_eq!(sim.canonical_snapshot().unwrap(), before);
    }

    #[test]
    fn generic_carry_has_atomic_validation_and_one_shared_capacity_for_all_goods() {
        let mut sim = sim();
        let actor = sim.host.state.ruler;
        let tools = game_workshop::site(&sim.host.storage, local_site(&sim))
            .expect("site exists")
            .tool;
        let opening = sim.canonical_snapshot().unwrap();
        for (source, quantity) in [
            (InventoryId(u64::MAX), Quantity(1_000)),
            (tools, Quantity(0)),
            (tools, Quantity(2_000)),
        ] {
            assert!(sim.carry_good(actor, source, quantity).is_err());
            assert_eq!(sim.canonical_snapshot().unwrap(), opening);
        }
        sim.carry_good(actor, tools, Quantity(1_000)).unwrap();
        let destination = match *sim.host.storage.activity(actor).expect("body exists") {
            Activity::Load { destination, .. } => destination,
            _ => panic!("carry must schedule loading"),
        };
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, destination)
                .expect("inventory exists")
                .quantity,
            Quantity(0)
        );
        sim.advance(MINUTE).unwrap();
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, destination)
                .expect("inventory exists")
                .quantity,
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
        let full = sim.canonical_snapshot().unwrap();
        let seed = sim
            .debug_add_inventory(
                Good::Seed,
                InventoryLocation::Area(AreaId(1)),
                Quantity(1_000),
                Quantity(1_000),
                BTreeSet::from([actor]),
            )
            .unwrap();
        let before = sim.canonical_snapshot().unwrap();
        assert!(sim.carry_good(actor, seed, Quantity(1_000)).is_err());
        assert_eq!(sim.canonical_snapshot().unwrap(), before);
        assert!(
            game_cargo::inventories(&sim.host.storage).count()
                > full.components.columns["cargo/inventory"].len()
        );
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
        let actor = sim.host.state.ruler;
        let source = game_cargo::inventories(&sim.host.storage)
            .map(|(_, row)| row)
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
        assert_eq!(sim.canonical_state(), restored.canonical_state());
        let destination = game_cargo::inventories(&sim.host.storage)
            .map(|(_, row)| row)
            .find(|stock| {
                stock.good == Good::Tool && stock.location == InventoryLocation::Carried(actor)
            })
            .unwrap()
            .id;
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, destination)
                .expect("inventory exists")
                .quantity,
            Quantity(1_000)
        );
        assert_eq!(
            sim.host.state.property.as_ref().unwrap().portions[&destination]
                .values()
                .sum::<u64>(),
            1_000
        );
    }
    #[test]
    fn denied_native_record_or_contact_load_preserves_job_inventory_and_ids() {
        let mut sim = sim();
        let actor = sim.host.state.ruler;
        let site = local_site(&sim);
        sim.debug_actor_command(actor, Command::Work { site })
            .unwrap();
        let records = sim
            .debug_add_inventory(
                Good::Record,
                InventoryLocation::Area(AreaId(1)),
                Quantity(1000),
                Quantity(1000),
                BTreeSet::from([actor]),
            )
            .unwrap();
        let before = sim.canonical_snapshot().unwrap();
        assert_eq!(
            sim.carry_good(actor, records, Quantity(1000)),
            Err(Error::Unavailable)
        );
        assert_eq!(sim.canonical_snapshot().unwrap(), before);
        let held = sim
            .debug_add_inventory(
                Good::Timber,
                InventoryLocation::Carried(actor),
                Quantity(1000),
                Quantity(10000),
                BTreeSet::from([actor]),
            )
            .unwrap();
        let recipient = sim.host.state.people[1].id;
        let before = sim.canonical_snapshot().unwrap();
        assert_eq!(
            sim.start_contact_delivery(actor, held, recipient, Quantity(1000)),
            Err(Error::Unavailable)
        );
        assert_eq!(sim.canonical_snapshot().unwrap(), before);
    }

    #[test]
    fn native_carry_and_unload_conserve_goods_and_replay_pending_allocation_once() {
        let mut sim = sim();
        let actor = sim.host.state.ruler;
        let source = game_workshop::site(&sim.host.storage, local_site(&sim))
            .expect("site exists")
            .tool;
        let opening = game_cargo::inventory(&sim.host.storage, source)
            .expect("inventory exists")
            .quantity;
        let carry = Submission {
            caller_sequence: sim.next_caller_sequence().unwrap(),
            command: Command::Carry {
                source,
                quantity: Quantity(1000),
            },
        };
        let receipt = sim.submit(carry.clone()).unwrap();
        let pending = sim.canonical_snapshot().unwrap();
        assert_eq!(sim.submit(carry).unwrap(), receipt);
        assert_eq!(sim.canonical_snapshot().unwrap(), pending);
        let destination = match *sim.host.storage.activity(actor).expect("body exists") {
            Activity::Load { destination, .. } => destination,
            _ => panic!("actual timed load"),
        };
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, source)
                .expect("inventory exists")
                .quantity,
            opening
        );
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, destination)
                .expect("inventory exists")
                .quantity,
            Quantity(0)
        );
        let mut saved = Simulation::load(
            &sim.save().unwrap(),
            Execution {
                workers: 4,
                reverse_jobs: true,
            },
        )
        .unwrap();
        sim.advance(MINUTE).unwrap();
        saved.advance(SimDuration(17000)).unwrap();
        saved.advance(SimDuration(43000)).unwrap();
        assert_eq!(
            sim.canonical_snapshot().unwrap(),
            saved.canonical_snapshot().unwrap()
        );
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, source)
                .expect("inventory exists")
                .quantity
                .0
                + game_cargo::inventory(&sim.host.storage, destination)
                    .expect("inventory exists")
                    .quantity
                    .0,
            opening.0
        );
        let unload = Submission {
            caller_sequence: sim.next_caller_sequence().unwrap(),
            command: Command::Load {
                source: destination,
                destination: source,
                quantity: Quantity(1000),
            },
        };
        let receipt = sim.submit(unload.clone()).unwrap();
        let pending = sim.canonical_snapshot().unwrap();
        assert_eq!(sim.submit(unload).unwrap(), receipt);
        assert_eq!(sim.canonical_snapshot().unwrap(), pending);
        sim.advance(MINUTE).unwrap();
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, source)
                .expect("inventory exists")
                .quantity,
            opening
        );
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, destination)
                .expect("inventory exists")
                .quantity,
            Quantity(0)
        );
        assert_eq!(sim.host.events().iter().filter(|event| matches!(event.event,
            EventKind::Transferred { actor: who, source: from, destination: to, .. }
                if who == actor && (from == source && to == destination || from == destination && to == source))).count(), 2);
    }
    #[test]
    fn ordered_cargo_phase_preserves_actual_mixed_water_metadata_between_receipts() {
        let mut sim = sim();
        let actor = sim.host.state.ruler;
        let source = sim
            .debug_add_inventory(
                Good::Water,
                InventoryLocation::Area(AreaId(1)),
                Quantity(2000),
                Quantity(3000),
                BTreeSet::from([actor]),
            )
            .unwrap();
        let mixed = sim
            .debug_add_inventory(
                Good::Water,
                InventoryLocation::Area(AreaId(1)),
                Quantity(1000),
                Quantity(3000),
                BTreeSet::from([actor]),
            )
            .unwrap();
        let destination = sim
            .debug_add_inventory(
                Good::Water,
                InventoryLocation::Area(AreaId(1)),
                Quantity(0),
                Quantity(3000),
                BTreeSet::from([actor]),
            )
            .unwrap();
        sim.collected_water_quality(source, Quantity(2000), 600);
        // The clean stock intentionally has no water-lot entry yet. Its real
        // existing volume must be observed by the first receipt's metadata hook.
        assert!(game_health::water_lot(&sim.host.storage, mixed).is_none());
        sim.debug_schedule_transfers(
            SimTime(MINUTE.0),
            vec![
                TransferIntent {
                    actor,
                    source,
                    destination: mixed,
                    quantity: Quantity(1000),
                },
                TransferIntent {
                    actor,
                    source: mixed,
                    destination,
                    quantity: Quantity(1000),
                },
            ],
        )
        .unwrap();
        let mut saved = Simulation::load(
            &sim.save().unwrap(),
            Execution {
                workers: 4,
                reverse_jobs: true,
            },
        )
        .unwrap();
        sim.advance(MINUTE).unwrap();
        saved.advance(SimDuration(17000)).unwrap();
        saved.advance(SimDuration(43000)).unwrap();
        assert_eq!(
            sim.canonical_snapshot().unwrap(),
            saved.canonical_snapshot().unwrap()
        );
        for inventory in [source, mixed, destination] {
            assert_eq!(
                game_cargo::inventory(&sim.host.storage, inventory)
                    .expect("inventory exists")
                    .quantity,
                Quantity(1000)
            );
            assert_eq!(
                game_health::water_lot(&sim.host.storage, inventory)
                    .expect("water lot exists")
                    .quantity(),
                1000
            );
        }
        assert_eq!(
            game_health::water_lot(&sim.host.storage, source)
                .expect("water lot exists")
                .load(),
            600000
        );
        assert_eq!(
            game_health::water_lot(&sim.host.storage, mixed)
                .expect("water lot exists")
                .load(),
            300000
        );
        assert_eq!(
            game_health::water_lot(&sim.host.storage, destination)
                .expect("water lot exists")
                .load(),
            300000
        );
        assert_eq!(
            [source, mixed, destination]
                .iter()
                .map(|id| game_health::water_lot(&sim.host.storage, *id)
                    .expect("water lot exists")
                    .load())
                .sum::<u128>(),
            1200000
        );
        assert_eq!(sim.host.events().iter().filter(|event| matches!(event.event,
            EventKind::Transferred { source: from, destination: to, .. } if from == source && to == mixed || from == mixed && to == destination)).count(), 2);
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
}
