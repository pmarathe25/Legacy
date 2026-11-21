//! Initialize the interval owners from a fresh authored scenario.
use super::*;

impl CoreHostSnapshot {
    /// Convert a fresh scenario while refusing to discard owners the host does not run.
    pub fn from_scenario(mut state: crate::State) -> Result<Self, String> {
        if state.physical.custody.tick != 0
            || state.physical.work_interval.is_some()
            || !state.physical.work.is_empty()
        {
            return Err("interval scenario import requires a fresh world".into());
        }
        state.people.par_sort_unstable_by_key(|person| person.id);
        state
            .work_tools
            .par_sort_unstable_by_key(|row| (row.recipe, row.good));
        if let Some(wear) = &mut state.tool_wear
            && let Some(highest) = state
                .physical
                .custody
                .containers
                .par_iter()
                .flat_map_iter(|holder| holder.lots.iter().filter_map(|lot| lot.item))
                .max()
        {
            wear.next_item = wear
                .next_item
                .max(highest.checked_add(1).ok_or("item identity overflow")?);
        }
        validate_authored_links(&state)?;
        let mut unsupported = Vec::new();
        if state
            .dynamics
            .as_ref()
            .is_some_and(|owner| owner.disease.is_none() && !owner.treatment.is_empty())
        {
            unsupported.push("treatment without disease");
        }
        if !unsupported.is_empty() {
            return Err(format!(
                "interval host does not yet support scenario owners: {}",
                unsupported.join(", ")
            ));
        }
        let expeditions = state
            .missions
            .take()
            .map(|owner| {
                game_economy_expeditions::elapsed::Expeditions::from_fresh(
                    owner,
                    state.physical.tick_millis,
                )
            })
            .transpose()?;
        let campaign = state
            .campaign
            .take()
            .map(|owner| {
                game_economy_campaign::elapsed::ElapsedCampaign::from_fresh(
                    owner,
                    state.physical.tick_millis,
                )
            })
            .transpose()?;
        let mut conflict = CorePersonalConflict {
            confinement: state.confinement.take().map(CoreConfinement::new),
            ..Default::default()
        };
        if let Some(owner) = state.combat.take() {
            let (tactical, wounds) =
                super::tactical::import_tactical(owner, state.physical.tick_millis)?;
            conflict.tactical = Some(tactical);
            conflict.wound_recovery_millis = wounds
                .as_ref()
                .map(|wounds| {
                    wounds
                        .rules
                        .recovery_ticks
                        .checked_mul(state.physical.tick_millis)
                        .ok_or("wound duration overflow")
                })
                .transpose()?;
            conflict.wounds = wounds;
        }
        let mut construction = state.construction.take();
        let mut maintenance = construction
            .as_mut()
            .and_then(|owner| owner.maintenance.take());
        if let Some(owner) = &mut maintenance {
            owner.normalize();
        }
        let sites = match (state.work_sites.take(), maintenance) {
            (Some(mut facilities), maintenance) => {
                facilities.normalize()?;
                Some(CoreSites {
                    facilities,
                    maintenance,
                })
            }
            (None, Some(_)) => return Err("site maintenance requires facilities".into()),
            (None, None) => None,
        };
        let (social, health, claims, personality) = if let Some(dynamics) = state.dynamics.take() {
            let social = dynamics.social.map(|owner| CoreSocial {
                owner,
                last_tick: None,
                language: None,
                signals: None,
            });
            let health = dynamics.disease.map(|owner| CoreHealth {
                owner,
                treatment: dynamics.treatment,
                last_tick: None,
            });
            (social, health, dynamics.claims, dynamics.personality)
        } else {
            (None, None, None, None)
        };
        let food_search = state.food_search.take().map(|search| CoreFoodSearch {
            search,
            knowledge: None,
            claims: None,
        });
        Ok(Self {
            mechanics: None,
            expeditions,
            personal_conflict: (conflict.tactical.is_some()
                || conflict.confinement.is_some()
                || campaign.as_ref().is_some_and(|owner| owner.costs.is_some()))
            .then_some(conflict),
            campaign,
            core: CoreSnapshot {
                seed: state.seed,
                physical: state.physical,
                boundary_millis: None,
                people: state.people,
                rules: state.rules,
                travel: state.travel,
                nutrition: state.nutrition,
                pending_food: state.pending_food,
            },
            autowork: state.autowork,
            tool_wear: state.tool_wear,
            tool_requirements: state.work_tools,
            paused_work_rates: vec![],
            pending_vital_deaths: vec![],
            fatigue: state
                .fatigue
                .map(|fatigue| game_economy_fatigue::TimedFatigue {
                    fatigue,
                    rate_period_millis: None,
                    remainders: vec![],
                    elapsed_damage_remainders: None,
                }),
            activity: vec![],
            food_search,
            claims,
            social,
            personality,
            knowledge: state.knowledge,
            health,
            medical: state.medical,
            giving: state.giving,
            practice_work_asks: vec![],
            practice_work_evidence: Default::default(),
            skills: state.skills.map(|owner| CoreSkills {
                owner,
                practiced: vec![],
                completed: vec![],
            }),
            taking: state.taking,
            agriculture: state.agriculture,
            irrigation: state.irrigation,
            environment: state.environment,
            recovery: state.recovery,
            purpose: state.purpose.map(|owner| CorePurpose {
                owner,
                progress: vec![],
            }),
            gathering: state.gathering,
            fauna: state.fauna,
            hunting: state.hunting,
            animal_feelings: state.animal_feelings,
            animal_care: state.animal_care,
            animal_products: state.animal_products,
            animal_restraints: state.animal_restraints,
            animal_handling: state.animal_handling,
            family: state.family,
            work_capabilities: None,
            dependent_care: state.care,
            opinions: state.opinions,
            goals: state.goals,
            dreams: state.dreams,
            market: state.market,
            writing: state.writing,
            local_transport: state.local_transport,
            construction,
            hauling: state.hauling,
            shared_work: state.shared_work.map(|owner| CoreSharedWork {
                owner,
                sessions: vec![],
                records: vec![],
            }),
            goal_setbacks: vec![],
            agreements: state.agreements,
            sites,
            schedule: None,
        })
    }
}

fn validate_authored_links(state: &crate::State) -> Result<(), String> {
    use game_lots::Place;

    let people = &state.people;
    if state
        .physical
        .custody
        .containers
        .par_iter()
        .any(|holder| match holder.place {
            Place::Person(id) => people
                .binary_search_by_key(&id, |person| person.id)
                .is_err(),
            Place::Inventory(_) => state.local_transport.is_none(),
            Place::Site(_) => state.construction.is_none(),
            _ => false,
        })
    {
        return Err("authored custody requires a known owner".into());
    }
    if state.fauna.as_ref().is_some_and(|fauna| {
        fauna.animals.par_iter().any(|animal| {
            animal.opinions.iter().any(|opinion| {
                people
                    .binary_search_by_key(&opinion.person, |person| person.id)
                    .is_err()
            })
        })
    }) {
        return Err("animal opinion references an unknown person".into());
    }
    if let Some(agreements) = &state.agreements
        && agreements
            .offering
            .par_iter()
            .any(|row| row.after_work_recipe.is_some())
    {
        let recipes: HashSet<_> = state
            .physical
            .recipes
            .par_iter()
            .map(|recipe| recipe.id)
            .collect();
        if agreements.offering.par_iter().any(|row| {
            row.after_work_recipe
                .is_some_and(|id| !recipes.contains(&id))
        }) {
            return Err("unknown remembered-work recipe".into());
        }
    }
    if let Some(missions) = &state.missions {
        let stores: HashSet<_> = state
            .local_transport
            .as_ref()
            .filter(|local| local.walking.is_some())
            .into_iter()
            .flat_map(|local| local.stores.iter().map(|store| store.id))
            .collect();
        if missions.entries.par_iter().any(|mission| {
            mission
                .delivery_preference
                .and_then(|value| value.acquisition)
                .and_then(|choice| choice.known_store)
                .is_some_and(|id| !stores.contains(&id))
        }) {
            return Err("delivery acquisition references an unknown local store".into());
        }
    }
    Ok(())
}
