//! Import an authored, fresh whole-boundary scenario into the interval host.
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
        let mut unsupported = Vec::new();
        for (name, present) in [
            ("confinement", state.confinement.is_some()),
            ("missions", state.missions.is_some()),
            ("campaign", state.campaign.is_some()),
            ("combat", state.combat.is_some()),
        ] {
            if present {
                unsupported.push(name);
            }
        }
        if state.construction.as_ref().is_some_and(|owner| {
            !owner.demolition.is_empty()
                || !owner.demolition_events.is_empty()
                || !owner.goal_builders.is_empty()
                || !owner.salvaging.is_empty()
                || !owner.salvage_events.is_empty()
        }) {
            unsupported.push("construction");
        }
        if state
            .local_transport
            .as_ref()
            .is_some_and(|owner| owner.walking.is_some() || !owner.goal_fetchers.is_empty())
        {
            unsupported.push("local_transport");
        }
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
        let mut construction = state.construction.take();
        let maintenance = construction
            .as_mut()
            .and_then(|owner| owner.maintenance.take());
        let sites = match (state.work_sites.take(), maintenance) {
            (Some(facilities), maintenance) => Some(CoreSites {
                facilities,
                maintenance,
            }),
            (None, Some(_)) => return Err("site maintenance requires facilities".into()),
            (None, None) => None,
        };
        let (social, health, claims, personality) = if let Some(dynamics) = state.dynamics.take() {
            let social = dynamics.social.map(|owner| CoreSocial {
                owner,
                last_tick: None,
                language: None,
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
            core: CoreSnapshot {
                seed: state.seed,
                physical: state.physical,
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
            fatigue: state
                .fatigue
                .map(|fatigue| game_economy_fatigue::TimedFatigue {
                    fatigue,
                    remainders: vec![],
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
