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
            ("market", state.market.is_some()),
            ("shared_work", state.shared_work.is_some()),
            ("confinement", state.confinement.is_some()),
            ("dreams", state.dreams.is_some()),
            ("missions", state.missions.is_some()),
            ("campaign", state.campaign.is_some()),
            ("skills", state.skills.is_some()),
            ("writing", state.writing.is_some()),
            ("combat", state.combat.is_some()),
            ("taking", state.taking.is_some()),
            ("giving", state.giving.is_some()),
            ("medical", state.medical.is_some()),
            ("hauling", state.hauling.is_some()),
            ("local_transport", state.local_transport.is_some()),
        ] {
            if present {
                unsupported.push(name);
            }
        }
        if state
            .dynamics
            .as_ref()
            .is_some_and(|owner| owner.personality.is_some())
        {
            unsupported.push("personality");
        }
        if state.construction.as_ref().is_some_and(|owner| {
            !owner.blueprints.is_empty()
                || !owner.policies.is_empty()
                || !owner.events.is_empty()
                || !owner.demolition.is_empty()
                || !owner.demolition_events.is_empty()
                || !owner.goal_builders.is_empty()
                || !owner.materials.is_empty()
                || !owner.salvaging.is_empty()
                || !owner.salvage_events.is_empty()
        }) {
            unsupported.push("construction");
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
        let maintenance = state
            .construction
            .take()
            .and_then(|owner| owner.maintenance);
        let sites = match (state.work_sites.take(), maintenance) {
            (Some(facilities), maintenance) => Some(CoreSites {
                facilities,
                maintenance,
            }),
            (None, Some(_)) => return Err("site maintenance requires facilities".into()),
            (None, None) => None,
        };
        let (social, health, claims) = if let Some(dynamics) = state.dynamics.take() {
            let social = dynamics.social.map(|owner| CoreSocial {
                owner,
                last_tick: None,
            });
            let health = dynamics.disease.map(|owner| CoreHealth {
                owner,
                treatment: dynamics.treatment,
                last_tick: None,
            });
            (social, health, dynamics.claims)
        } else {
            (None, None, None)
        };
        let food_search = state.food_search.take().map(|search| CoreFoodSearch {
            search,
            knowledge: None,
            claims: None,
        });
        Ok(Self {
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
            knowledge: state.knowledge,
            health,
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
            agreements: state.agreements,
            sites,
            schedule: None,
        })
    }
}
