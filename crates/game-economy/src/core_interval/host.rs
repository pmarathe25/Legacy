//! A saved host for the limited fractional-time core and its personal owners.
use super::*;
use game_autowork::Autowork;
use game_economy_work::tool_wear::ToolWear;
mod agriculture;
mod animal_care;
mod animal_handling;
mod animal_products;
mod animal_restraints;
mod claims;
mod construction;
mod dependent_care;
mod dreams;
mod family;
mod fatigue;
mod fauna;
mod food_search;
mod gathering;
mod giving;
mod goals;
mod hauling;
mod hauling_requests;
mod hunting;
mod local_transport;
mod market;
mod mechanics;
mod medical;
mod offers;
mod personal_choice;
mod personality;
mod purpose;
mod requests;
mod scenario;
mod scheduler;
mod shared_work;
mod sites;
mod skills;
mod social;
mod taking;
mod tools;
mod treatment;
mod writing;
pub use construction::CoreConstructionResults;
pub use food_search::{CoreFoodProvision, CoreFoodSearch};
pub use giving::CorePracticeWorkEvidence;
pub use hauling::CoreHaulingResults;
pub use local_transport::CoreLocalTransportResults;
pub use market::CoreMarketResults;
pub use mechanics::{
    CoreMechanics, MechanicalCapability, MechanicalMaterial, MechanicalMemory, MechanicalPolicy,
};
pub use offers::CoreSocialResults;
pub use personal_choice::{CoreChoiceEvent, CoreChoiceIntent, CoreChoiceOutcome, CoreWorkIntent};
pub use purpose::CorePurpose;
pub use requests::{CoreDeliveryResults, CoreRequestResults};
pub use scheduler::{CoreRun, CoreRunEvent, CoreSchedule, DecisionPhase};
pub use shared_work::{CoreAssistRecord, CoreAssistSession, CoreSharedWork};
pub use sites::{CoreRepairResults, CoreSites};
pub use skills::{CoreSkillCredit, CoreSkills};
pub use social::{CoreDynamicsResults, CoreHealth, CoreSocial};
pub use tools::CoreToolRepairResults;
pub use treatment::CoreTreatmentResults;
pub use writing::{CoreWritingPracticeResults, CoreWritingResults};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreHostSnapshot {
    pub core: CoreSnapshot,
    #[serde(default)]
    pub mechanics: Option<CoreMechanics>,
    #[serde(default)]
    pub autowork: Option<Autowork>,
    #[serde(default)]
    pub tool_wear: Option<ToolWear>,
    #[serde(default)]
    pub tool_requirements: Vec<game_economy_work::tools::Requirement>,
    #[serde(default, alias = "tool_paused_rates")]
    pub paused_work_rates: Vec<(u64, u64)>,
    #[serde(default)]
    pub fatigue: Option<game_economy_fatigue::TimedFatigue>,
    #[serde(default)]
    pub activity: Vec<game_economy_fatigue::TimedActivity>,
    #[serde(default)]
    pub food_search: Option<CoreFoodSearch>,
    #[serde(default)]
    pub claims: Option<game_economy_claims::Claims>,
    #[serde(default)]
    pub social: Option<CoreSocial>,
    #[serde(default)]
    pub personality: Option<game_economy_dynamics::personality::Personality>,
    #[serde(default)]
    pub knowledge: Option<game_economy_knowledge::Knowledge>,
    #[serde(default)]
    pub health: Option<CoreHealth>,
    #[serde(default)]
    pub medical: Option<crate::Medical>,
    #[serde(default)]
    pub giving: Option<crate::Giving>,
    #[serde(default)]
    pub practice_work_asks: Vec<game_autowork::requests::Request>,
    #[serde(default)]
    pub practice_work_evidence: CorePracticeWorkEvidence,
    #[serde(default)]
    pub skills: Option<CoreSkills>,
    #[serde(default)]
    pub taking: Option<game_economy_taking::Taking>,
    #[serde(default)]
    pub agriculture: Option<game_economy_agriculture::Agriculture>,
    #[serde(default)]
    pub irrigation: Option<game_economy_agriculture::irrigation::Irrigation>,
    #[serde(default)]
    pub environment: Option<game_environment::Environment>,
    #[serde(default)]
    pub recovery: Option<crate::RecoveryRules>,
    #[serde(default)]
    pub purpose: Option<CorePurpose>,
    #[serde(default)]
    pub gathering: Option<crate::Gathering>,
    #[serde(default)]
    pub fauna: Option<game_fauna::Fauna>,
    #[serde(default)]
    pub hunting: Option<crate::Hunting>,
    #[serde(default)]
    pub animal_feelings: Option<crate::AnimalFeelings>,
    #[serde(default)]
    pub animal_care: Option<crate::AnimalCare>,
    #[serde(default)]
    pub animal_products: Option<crate::AnimalProducts>,
    #[serde(default)]
    pub animal_restraints: Option<crate::AnimalRestraints>,
    #[serde(default)]
    pub animal_handling: Option<crate::AnimalHandling>,
    #[serde(default)]
    pub family: Option<game_economy_family::Families>,
    #[serde(default)]
    pub dependent_care: Option<crate::DependentCare>,
    #[serde(default)]
    pub opinions: Option<game_opinions::Opinions>,
    #[serde(default)]
    pub goals: Option<game_goals::Goals>,
    #[serde(default)]
    pub dreams: Option<game_economy_goals::Dreams>,
    #[serde(default)]
    pub shared_work: Option<CoreSharedWork>,
    #[serde(default)]
    pub market: Option<game_market::Market>,
    #[serde(default)]
    pub writing: Option<crate::Writing>,
    #[serde(default)]
    pub local_transport: Option<game_economy_local_transport::LocalTransport>,
    #[serde(default)]
    pub construction: Option<game_economy_building::Construction>,
    #[serde(default)]
    pub hauling: Option<crate::Hauling>,
    #[serde(default)]
    pub goal_setbacks: Vec<(u64, u32)>,
    #[serde(default)]
    pub agreements: Option<game_agreements::Agreements>,
    #[serde(default)]
    pub sites: Option<CoreSites>,
    #[serde(default)]
    pub schedule: Option<CoreSchedule>,
}

/// This host composes only the owners listed in `CoreHostSnapshot`.
pub struct CoreHost {
    core: CoreInterval,
    mechanics: Option<mechanics::Runtime>,
    autowork: Option<Autowork>,
    tool_wear: Option<ToolWear>,
    tool_requirements: Vec<game_economy_work::tools::Requirement>,
    paused_work_rates: Vec<(u64, u64)>,
    fatigue: Option<game_economy_fatigue::TimedFatigue>,
    activity: Vec<game_economy_fatigue::TimedActivity>,
    goal_setbacks: Vec<(u64, u32)>,
    food_search: Option<CoreFoodSearch>,
    claims: Option<game_economy_claims::Claims>,
    social: Option<CoreSocial>,
    personality: Option<game_economy_dynamics::personality::Personality>,
    knowledge: Option<game_economy_knowledge::Knowledge>,
    health: Option<CoreHealth>,
    medical: Option<crate::Medical>,
    giving: Option<crate::Giving>,
    practice_work_asks: Vec<game_autowork::requests::Request>,
    practice_work_evidence: CorePracticeWorkEvidence,
    skills: Option<CoreSkills>,
    taking: Option<game_economy_taking::Taking>,
    agriculture: Option<game_economy_agriculture::Agriculture>,
    irrigation: Option<game_economy_agriculture::irrigation::Irrigation>,
    environment: Option<game_environment::Environment>,
    recovery: Option<crate::RecoveryRules>,
    purpose: Option<CorePurpose>,
    gathering: Option<crate::Gathering>,
    fauna: Option<game_fauna::Fauna>,
    hunting: Option<crate::Hunting>,
    animal_feelings: Option<crate::AnimalFeelings>,
    animal_care: Option<crate::AnimalCare>,
    animal_products: Option<crate::AnimalProducts>,
    animal_restraints: Option<crate::AnimalRestraints>,
    animal_handling: Option<crate::AnimalHandling>,
    family: Option<game_economy_family::Families>,
    dependent_care: Option<crate::DependentCare>,
    skill_rates: Option<game_economy_skills::Index>,
    skill_practice_recipes: HashSet<u32>,
    personality_refusal_tick: Option<u64>,
    personality_refusals: Vec<u64>,
    work_checked_revision: Option<u64>,
    work_checked_blocked: Vec<u64>,
    opinions: Option<game_opinions::Opinions>,
    goals: Option<game_goals::Goals>,
    dreams: Option<game_economy_goals::Dreams>,
    shared_work: Option<CoreSharedWork>,
    market: Option<game_market::Market>,
    writing: Option<crate::Writing>,
    local_transport: Option<game_economy_local_transport::LocalTransport>,
    construction: Option<game_economy_building::Construction>,
    hauling: Option<crate::Hauling>,
    agreements: Option<game_agreements::Agreements>,
    sites: Option<CoreSites>,
    schedule: Option<CoreSchedule>,
}
struct CoreRuntimeOptions {
    mechanics: Option<CoreMechanics>,
    sites: Option<CoreSites>,
    schedule: Option<CoreSchedule>,
    tool_requirements: Vec<game_economy_work::tools::Requirement>,
    paused_work_rates: Vec<(u64, u64)>,
    fatigue: Option<game_economy_fatigue::TimedFatigue>,
    activity: Vec<game_economy_fatigue::TimedActivity>,
    goal_setbacks: Vec<(u64, u32)>,
    food_search: Option<CoreFoodSearch>,
    claims: Option<game_economy_claims::Claims>,
    social: Option<CoreSocial>,
    personality: Option<game_economy_dynamics::personality::Personality>,
    knowledge: Option<game_economy_knowledge::Knowledge>,
    health: Option<CoreHealth>,
    medical: Option<crate::Medical>,
    giving: Option<crate::Giving>,
    practice_work_asks: Vec<game_autowork::requests::Request>,
    practice_work_evidence: CorePracticeWorkEvidence,
    skills: Option<CoreSkills>,
    taking: Option<game_economy_taking::Taking>,
    agriculture: Option<game_economy_agriculture::Agriculture>,
    irrigation: Option<game_economy_agriculture::irrigation::Irrigation>,
    environment: Option<game_environment::Environment>,
    recovery: Option<crate::RecoveryRules>,
    purpose: Option<CorePurpose>,
    gathering: Option<crate::Gathering>,
    fauna: Option<game_fauna::Fauna>,
    hunting: Option<crate::Hunting>,
    animal_feelings: Option<crate::AnimalFeelings>,
    animal_care: Option<crate::AnimalCare>,
    animal_products: Option<crate::AnimalProducts>,
    animal_restraints: Option<crate::AnimalRestraints>,
    animal_handling: Option<crate::AnimalHandling>,
    family: Option<game_economy_family::Families>,
    dependent_care: Option<crate::DependentCare>,
    dreams: Option<game_economy_goals::Dreams>,
    shared_work: Option<CoreSharedWork>,
    market: Option<game_market::Market>,
    writing: Option<crate::Writing>,
    local_transport: Option<game_economy_local_transport::LocalTransport>,
    construction: Option<game_economy_building::Construction>,
    hauling: Option<crate::Hauling>,
}
struct EffectiveWork {
    policy: Option<Autowork>,
    goal_candidates: Vec<(u64, u32)>,
}

impl CoreHost {
    pub fn begin(snapshot: CoreHostSnapshot, rates: &[(u64, u64)]) -> Result<Self, String> {
        let CoreHostSnapshot {
            core,
            mechanics,
            autowork,
            tool_wear,
            tool_requirements,
            paused_work_rates,
            fatigue,
            activity,
            goal_setbacks,
            food_search,
            claims,
            social,
            personality,
            knowledge,
            health,
            medical,
            giving,
            practice_work_asks,
            practice_work_evidence,
            skills,
            taking,
            agriculture,
            irrigation,
            environment,
            recovery,
            purpose,
            gathering,
            fauna,
            hunting,
            animal_feelings,
            animal_care,
            animal_products,
            animal_restraints,
            animal_handling,
            family,
            dependent_care,
            opinions,
            goals,
            dreams,
            shared_work,
            market,
            writing,
            local_transport,
            construction,
            hauling,
            agreements,
            sites,
            schedule,
        } = snapshot;
        let skill_rates = skills
            .as_ref()
            .map(|owner| owner.initial_rates(&core, rates))
            .transpose()?;
        let core = CoreInterval::begin(core, skill_rates.as_deref().unwrap_or(rates))?;
        Self::compile(
            core,
            autowork,
            tool_wear,
            opinions,
            goals,
            agreements,
            CoreRuntimeOptions {
                mechanics,
                dreams,
                shared_work,
                market,
                writing,
                local_transport,
                construction,
                hauling,
                sites,
                schedule,
                tool_requirements,
                paused_work_rates,
                fatigue,
                activity,
                goal_setbacks,
                food_search,
                claims,
                social,
                personality,
                knowledge,
                health,
                medical,
                giving,
                practice_work_asks,
                practice_work_evidence,
                skills,
                taking,
                agriculture,
                irrigation,
                environment,
                recovery,
                purpose,
                gathering,
                fauna,
                hunting,
                animal_feelings,
                animal_care,
                animal_products,
                animal_restraints,
                animal_handling,
                family,
                dependent_care,
            },
        )
    }
    pub fn resume(snapshot: CoreHostSnapshot) -> Result<Self, String> {
        let CoreHostSnapshot {
            core,
            mechanics,
            autowork,
            tool_wear,
            tool_requirements,
            paused_work_rates,
            fatigue,
            activity,
            goal_setbacks,
            food_search,
            claims,
            social,
            personality,
            knowledge,
            health,
            medical,
            giving,
            practice_work_asks,
            practice_work_evidence,
            skills,
            taking,
            agriculture,
            irrigation,
            environment,
            recovery,
            purpose,
            gathering,
            fauna,
            hunting,
            animal_feelings,
            animal_care,
            animal_products,
            animal_restraints,
            animal_handling,
            family,
            dependent_care,
            opinions,
            goals,
            dreams,
            shared_work,
            market,
            writing,
            local_transport,
            construction,
            hauling,
            agreements,
            sites,
            schedule,
        } = snapshot;
        let core = CoreInterval::resume(core)?;
        Self::compile(
            core,
            autowork,
            tool_wear,
            opinions,
            goals,
            agreements,
            CoreRuntimeOptions {
                mechanics,
                dreams,
                shared_work,
                market,
                writing,
                local_transport,
                construction,
                hauling,
                sites,
                schedule,
                tool_requirements,
                paused_work_rates,
                fatigue,
                activity,
                goal_setbacks,
                food_search,
                claims,
                social,
                personality,
                knowledge,
                health,
                medical,
                giving,
                practice_work_asks,
                practice_work_evidence,
                skills,
                taking,
                agriculture,
                irrigation,
                environment,
                recovery,
                purpose,
                gathering,
                fauna,
                hunting,
                animal_feelings,
                animal_care,
                animal_products,
                animal_restraints,
                animal_handling,
                family,
                dependent_care,
            },
        )
    }
    fn compile(
        core: CoreInterval,
        autowork: Option<Autowork>,
        tool_wear: Option<ToolWear>,
        opinions: Option<game_opinions::Opinions>,
        goals: Option<game_goals::Goals>,
        agreements: Option<game_agreements::Agreements>,
        options: CoreRuntimeOptions,
    ) -> Result<Self, String> {
        let CoreRuntimeOptions {
            mechanics,
            dreams,
            shared_work,
            market,
            mut writing,
            mut local_transport,
            mut construction,
            hauling,
            mut sites,
            schedule,
            tool_requirements,
            paused_work_rates,
            fatigue,
            mut activity,
            goal_setbacks,
            mut food_search,
            mut claims,
            social,
            personality,
            mut knowledge,
            health,
            medical,
            giving,
            practice_work_asks,
            practice_work_evidence,
            skills,
            taking,
            agriculture,
            irrigation,
            environment,
            recovery,
            purpose,
            gathering,
            fauna,
            hunting,
            mut animal_feelings,
            animal_care,
            mut animal_products,
            mut animal_restraints,
            mut animal_handling,
            family,
            dependent_care,
        } = options;
        if let Some(legacy) = food_search.as_mut().and_then(|owner| owner.claims.take()) {
            if claims.is_some() {
                return Err("duplicate personal claim owners".into());
            }
            claims = Some(legacy);
        }
        if let Some(legacy) = food_search
            .as_mut()
            .and_then(|owner| owner.knowledge.take())
        {
            if knowledge.is_some() {
                return Err("duplicate personal knowledge owners".into());
            }
            knowledge = Some(legacy);
        }
        let mechanics = mechanics.map(|owner| owner.compile(&core)).transpose()?;
        if let Some(owner) = &autowork {
            let people: Vec<_> = core
                .people
                .par_iter()
                .map(|p| (p.id, p.region, p.health, p.hunger))
                .collect();
            let view = core.physical.view(core.physical.tick())?;
            let production =
                game_lot_projection::production(&core.physical, &view, core.rules.max_work_hunger);
            owner.validate(&people, &production)?;
            if owner.next_id == 0 {
                return Err("invalid autonomous work identity".into());
            }
        }
        if let Some(owner) = &tool_wear {
            let goods: HashSet<_> = core
                .physical
                .goods()
                .par_iter()
                .map(|good| good.id)
                .collect();
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            owner.validate(&core.physical, &goods, &people, core.physical.tick())?;
        }
        if !tool_requirements.is_empty() {
            let goods: HashSet<_> = core
                .physical
                .goods()
                .par_iter()
                .map(|good| good.id)
                .collect();
            let recipes: HashSet<_> = core
                .physical
                .recipes()
                .par_iter()
                .map(|recipe| recipe.id)
                .collect();
            game_economy_work::tools::validate(&tool_requirements, &recipes, &goods)?;
        }
        let rates = &core
            .physical
            .work_interval()
            .ok_or("no physical interval")?
            .rates;
        if (tool_requirements.is_empty()
            && fatigue.is_none()
            && health.is_none()
            && !paused_work_rates.is_empty())
            || paused_work_rates
                .par_windows(2)
                .any(|rows| rows[0].0 >= rows[1].0)
            || paused_work_rates.par_iter().any(|&(job, rate)| {
                rate == 0
                    || rates.binary_search_by_key(&job, |row| row.0).is_ok()
                    || core
                        .physical
                        .work()
                        .binary_search_by_key(&job, |row| row.job.id)
                        .ok()
                        .is_none_or(|index| {
                            core.physical.work()[index].phase != game_lot_runtime::Phase::Working
                        })
            })
        {
            return Err("invalid paused work".into());
        }
        if let Some(owner) = &fatigue {
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            owner.validate(&people, core.physical.tick_millis())?;
            let elapsed = core.elapsed_millis().ok_or("no physical interval")?;
            if activity.is_empty() && elapsed == 0 {
                activity = core
                    .people
                    .par_iter()
                    .map(|person| game_economy_fatigue::TimedActivity {
                        id: person.id,
                        ..Default::default()
                    })
                    .collect();
            }
            if activity.len() != core.people.len()
                || activity
                    .par_iter()
                    .zip(core.people.par_iter())
                    .any(|(row, person)| {
                        row.id != person.id
                            || row.alive_millis > elapsed
                            || u128::from(row.work_millis)
                                + u128::from(row.travel_millis)
                                + u128::from(row.rest_millis)
                                + u128::from(row.idle_millis)
                                > u128::from(row.alive_millis)
                    })
            {
                return Err("invalid saved fatigue activity".into());
            }
        } else if !activity.is_empty() {
            return Err("fatigue activity has no owner".into());
        }
        if let Some(owner) = &opinions {
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            owner
                .validate(&people, core.physical.tick())
                .map_err(str::to_owned)?;
        }
        if let Some(owner) = &knowledge {
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            let view = core.physical.view(core.physical.tick())?;
            game_economy_knowledge::validate(Some(owner), &people, &view, core.physical.tick())?;
        }
        if let Some(owner) = &food_search {
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            owner
                .search
                .validate(&people, knowledge.as_ref(), core.travel.as_ref())?;
        }
        if let Some(claims) = &claims {
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            let view = core.physical.view(core.physical.tick())?;
            claims.validate(&people, &view, core.physical.tick())?;
        }
        if let Some(owner) = &personality {
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            owner.validate(&people).map_err(str::to_owned)?;
        }
        if let Some(social) = &social {
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            social
                .owner
                .validate(&people, core.physical.tick())
                .map_err(str::to_owned)?;
            if let Some(language) = &social.language {
                language
                    .validate(&people, core.physical.tick())
                    .map_err(str::to_owned)?;
            }
            if let Some(signals) = &social.signals {
                signals
                    .validate(&people, core.physical.tick())
                    .map_err(str::to_owned)?;
                if signals.minds.par_iter().any(|mind| {
                    mind.gift_preferences.iter().any(|preference| {
                        !core
                            .physical
                            .goods()
                            .iter()
                            .any(|good| good.id == preference.good)
                    })
                }) {
                    return Err("unknown signal gift good".into());
                }
                if signals
                    .events
                    .par_iter()
                    .any(|event| Some(event.tick) != social.last_tick)
                {
                    return Err("invalid signal event time".into());
                }
            }
            if social
                .last_tick
                .is_some_and(|tick| tick == 0 || tick > core.physical.tick())
                || social
                    .owner
                    .events
                    .par_iter()
                    .any(|event| Some(event.tick) != social.last_tick)
            {
                return Err("invalid social decision time".into());
            }
        }
        if let Some(health) = &health {
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            health.owner.validate(&people, core.physical.tick())?;
            let goods: HashSet<_> = core
                .physical
                .goods()
                .par_iter()
                .map(|good| good.id)
                .collect();
            if health
                .owner
                .treatments
                .par_iter()
                .any(|treatment| !goods.contains(&treatment.medicine))
                || health
                    .last_tick
                    .is_some_and(|tick| tick == 0 || tick > core.physical.tick())
            {
                return Err("invalid disease decision state".into());
            }
            game_economy_treatment::validate(
                &health.treatment,
                Some(&health.owner),
                &people,
                &goods,
            )?;
        }
        if let Some(owner) = &skills {
            owner.validate(&core)?;
        }
        if let Some(owner) = &giving {
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            owner.validate(
                &people,
                core.physical.goods(),
                core.physical.recipes(),
                core.physical.tick(),
            )?;
        }
        if !practice_work_asks.is_empty() {
            let Some(requests) = autowork.as_ref().and_then(|auto| auto.requests.as_ref()) else {
                return Err("practice work asks require work requests".into());
            };
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            let recipes: HashSet<_> = core
                .physical
                .recipes()
                .par_iter()
                .map(|recipe| recipe.id)
                .collect();
            if giving
                .as_ref()
                .and_then(|owner| owner.practices.as_ref())
                .is_none()
                || practice_work_asks.par_iter().any(|ask| {
                    ask.id == 0
                        || ask.id >= requests.next_id
                        || !people.contains(&ask.requester)
                        || !people.contains(&ask.worker)
                        || ask.requester == ask.worker
                        || !recipes.contains(&ask.recipe)
                        || ask.created_at != core.physical.tick()
                        || ask.expires < ask.created_at
                })
                || practice_work_asks
                    .par_windows(2)
                    .any(|rows| rows[0].id >= rows[1].id)
            {
                return Err("invalid pending practice work asks".into());
            }
        }
        if practice_work_evidence != CorePracticeWorkEvidence::default() {
            if giving
                .as_ref()
                .and_then(|owner| owner.practices.as_ref())
                .is_none()
            {
                return Err("work evidence requires personal practices".into());
            }
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            let recipes: HashSet<_> = core
                .physical
                .recipes()
                .par_iter()
                .map(|recipe| recipe.id)
                .collect();
            let goods: HashSet<_> = core
                .physical
                .goods()
                .par_iter()
                .map(|good| good.id)
                .collect();
            let elapsed = core.elapsed_millis().ok_or("no physical interval")?;
            let mut ordered_offsets: Vec<_> = practice_work_evidence
                .start_times
                .par_iter()
                .map(|&(_, _, offset, order)| (order, offset))
                .collect();
            ordered_offsets.par_extend(
                practice_work_evidence
                    .gifts
                    .par_iter()
                    .map(|&(_, _, _, _, offset, order)| (order, offset)),
            );
            ordered_offsets.par_sort_unstable_by_key(|row| row.0);
            let mut timed_jobs: Vec<_> = practice_work_evidence
                .start_times
                .par_iter()
                .map(|&(job, _, _, _)| job)
                .collect();
            timed_jobs.par_sort_unstable();
            let mut recorded_jobs: Vec<_> = practice_work_evidence
                .starts
                .par_iter()
                .map(|row| row.job)
                .collect();
            recorded_jobs.par_sort_unstable();
            let mut start_ids: Vec<_> = practice_work_evidence
                .starts
                .par_iter()
                .map(|row| row.request)
                .collect();
            start_ids.par_sort_unstable();
            if ordered_offsets
                .par_windows(2)
                .any(|rows| rows[0].1 > rows[1].1)
                || ordered_offsets
                    .par_iter()
                    .enumerate()
                    .any(|(index, &(order, _))| order != index as u64)
                || timed_jobs != recorded_jobs
                || timed_jobs.par_windows(2).any(|rows| rows[0] == rows[1])
                || practice_work_evidence
                    .start_times
                    .par_iter()
                    .any(|&(_, tick, offset, _)| {
                        tick != core.physical.tick()
                            || offset >= core.physical.tick_millis()
                            || offset > elapsed
                    })
                || start_ids.par_windows(2).any(|rows| rows[0] == rows[1])
                || practice_work_evidence.starts.par_iter().any(|row| {
                    row.request == 0
                        || row.job == 0
                        || row.requester == row.worker
                        || !people.contains(&row.requester)
                        || !people.contains(&row.worker)
                        || !recipes.contains(&row.recipe)
                })
                || practice_work_evidence
                    .finished
                    .par_windows(2)
                    .any(|rows| rows[0] >= rows[1])
                || practice_work_evidence
                    .failed
                    .par_windows(2)
                    .any(|rows| rows[0] >= rows[1])
                || practice_work_evidence
                    .finished
                    .par_iter()
                    .any(|id| *id == 0 || practice_work_evidence.failed.binary_search(id).is_ok())
                || practice_work_evidence.failed.par_iter().any(|id| *id == 0)
                || practice_work_evidence.gifts.par_iter().any(
                    |&(recipient, giver, good, quantity, offset, _)| {
                        recipient == giver
                            || !people.contains(&recipient)
                            || !people.contains(&giver)
                            || !goods.contains(&good)
                            || quantity == 0
                            || offset >= core.physical.tick_millis()
                            || offset > elapsed
                    },
                )
            {
                return Err("invalid pending work practice evidence".into());
            }
        }
        if let Some(owner) = &taking {
            if owner.policies.par_iter().any(|policy| policy.escalate) {
                return Err("taking escalation is not yet supported by the interval host".into());
            }
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            let goods: HashSet<_> = core
                .physical
                .goods()
                .par_iter()
                .map(|good| good.id)
                .collect();
            owner.validate(&people, &goods, core.physical.tick())?;
        }
        if let Some(owner) = &medical {
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            let goods: HashSet<_> = core
                .physical
                .goods()
                .par_iter()
                .map(|good| good.id)
                .collect();
            owner.validate(
                &people,
                &goods,
                health.as_ref().map(|health| &health.owner),
                None,
                social.is_some(),
            )?;
        }
        if let Some(owner) = &agriculture {
            let view = core.physical.view(core.physical.tick())?;
            owner.validate(&core.people, core.physical.goods(), &view)?;
        }
        if let Some(owner) = &irrigation {
            let agriculture = agriculture
                .as_ref()
                .ok_or("irrigation requires agriculture")?;
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            let goods: HashSet<_> = core
                .physical
                .goods()
                .par_iter()
                .map(|good| good.id)
                .collect();
            owner.validate(agriculture, &people, &goods, core.physical.tick())?;
        }
        if environment.is_some() {
            let goods: HashSet<_> = core
                .physical
                .goods()
                .par_iter()
                .map(|good| good.id)
                .collect();
            let view = core.physical.view(core.physical.tick())?;
            crate::environment::validate(environment.as_ref(), &view, &goods)?;
        }
        if let Some(owner) = &gathering {
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            let goods: HashSet<_> = core
                .physical
                .goods()
                .par_iter()
                .map(|good| good.id)
                .collect();
            let view = core.physical.view(core.physical.tick())?;
            owner.validate(&people, &goods, &view)?;
        }
        if let Some(owner) = &fauna {
            let goods: HashSet<_> = core
                .physical
                .goods()
                .par_iter()
                .map(|good| good.id)
                .collect();
            let view = core.physical.view(core.physical.tick())?;
            let regions: HashSet<_> = view
                .containers
                .par_iter()
                .filter_map(|row| match row.place {
                    game_lots::Place::Stock { region, .. } => Some(region),
                    _ => None,
                })
                .collect();
            owner
                .validate(&regions, &goods, core.physical.tick())
                .map_err(str::to_owned)?;
        }
        if let Some(owner) = &mut animal_feelings {
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            owner.normalize_and_validate(&people, fauna.as_ref())?;
        }
        if let Some(owner) = &dependent_care {
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            game_economy_work::care::validate(
                owner,
                family.as_ref(),
                &people,
                core.physical.tick(),
            )?;
        }
        if let Some(owner) = &family {
            game_economy_family::validate(
                owner,
                &core.people,
                core.physical.tick(),
                core.rules.max_health,
            )?;
            game_economy_work::validate_age(
                &game_economy_work::underage(owner),
                core.physical.work(),
            )?;
        }
        if let Some(owner) = &mut animal_handling {
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            owner.validate(&people, fauna.as_ref(), core.physical.tick())?;
        }
        if let Some(owner) = &mut animal_restraints {
            owner.normalize_and_validate(&core.people, fauna.as_ref(), core.physical.tick())?;
        }
        if let Some(owner) = &mut animal_products {
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            let goods: HashSet<_> = core
                .physical
                .goods()
                .par_iter()
                .map(|good| good.id)
                .collect();
            owner.validate(&people, &goods, fauna.as_ref(), core.physical.tick())?;
        }
        if let Some(owner) = &animal_care {
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            let goods: HashSet<_> = core
                .physical
                .goods()
                .par_iter()
                .map(|good| good.id)
                .collect();
            owner.validate(&people, &goods, fauna.as_ref())?;
        }
        if let Some(owner) = &hunting {
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            let goods: HashSet<_> = core
                .physical
                .goods()
                .par_iter()
                .map(|good| good.id)
                .collect();
            owner.validate(&people, &goods, fauna.as_ref())?;
        }
        if let Some(owner) = &goals {
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            let recipes: HashSet<_> = core
                .physical
                .recipes()
                .par_iter()
                .map(|recipe| recipe.id)
                .collect();
            owner
                .validate(&people, &recipes, core.physical.tick())
                .map_err(str::to_owned)?;
        }
        if let Some(owner) = &hauling {
            let people: HashSet<_> = core.people.par_iter().map(|p| p.id).collect();
            let goods: HashSet<_> = core.physical.goods().par_iter().map(|g| g.id).collect();
            let view = core.physical.view(core.physical.tick())?;
            owner.validate(&people, &goods, &view, core.travel.as_ref())?;
        }
        if let Some(owner) = &mut local_transport {
            if owner.walking.is_some() || !owner.goal_fetchers.is_empty() {
                return Err(
                    "local walking and goal-directed carrying are not yet supported".into(),
                );
            }
            owner.normalize();
            let people: HashSet<_> = core.people.par_iter().map(|p| p.id).collect();
            let goods: HashSet<_> = core.physical.goods().par_iter().map(|g| g.id).collect();
            let view = core.physical.view(core.physical.tick())?;
            owner.validate(&people, &goods, &view, core.physical.tick())?;
        }
        if let Some(owner) = &mut writing {
            if (!owner.authors.is_empty()
                || owner.pages.par_iter().any(|page| page.claim.is_some()))
                && knowledge.is_none()
            {
                return Err("written claims require personal knowledge".into());
            }
            let people: HashSet<_> = core.people.par_iter().map(|p| p.id).collect();
            let goods: HashSet<_> = core.physical.goods().par_iter().map(|g| g.id).collect();
            let view = core.physical.view(core.physical.tick())?;
            let regions: HashSet<_> = view
                .containers
                .par_iter()
                .filter_map(|row| match row.place {
                    game_lots::Place::Stock { region, .. } => Some(region),
                    _ => None,
                })
                .collect();
            owner.normalize_and_validate(&people, &goods, &regions, &core.physical.snapshot())?;
        }
        if let Some(owner) = &market {
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            let goods: HashSet<_> = core
                .physical
                .goods()
                .par_iter()
                .map(|good| good.id)
                .collect();
            owner.validate(&people, &goods, core.physical.tick())?;
        }
        if let Some(owner) = &shared_work {
            owner.validate(&core, sites.as_ref())?;
        }
        if let Some(owner) = &dreams {
            if goals.is_none() {
                return Err("aspirations require personal goals".into());
            }
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            let recipes: HashSet<_> = core
                .physical
                .recipes()
                .par_iter()
                .map(|recipe| recipe.id)
                .collect();
            owner
                .validate(&people, &recipes, core.physical.tick())
                .map_err(str::to_owned)?;
        }
        if let Some(owner) = &agreements {
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            let goods: HashSet<_> = core
                .physical
                .goods()
                .par_iter()
                .map(|good| good.id)
                .collect();
            owner
                .validate(&people, &goods, core.physical.tick())
                .map_err(str::to_owned)?;
        }
        if let Some(owner) = &purpose {
            let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
            owner.validate(&people)?;
        }
        if !goal_setbacks.is_empty()
            && (goals.is_none()
                || goal_setbacks.par_windows(2).any(|rows| rows[0] >= rows[1])
                || goal_setbacks.par_iter().any(|&(person, recipe)| {
                    core.people
                        .binary_search_by_key(&person, |row| row.id)
                        .is_err()
                        || core
                            .physical
                            .recipes()
                            .binary_search_by_key(&recipe, |row| row.id)
                            .is_err()
                }))
        {
            return Err("invalid pending goal setbacks".into());
        }
        if recovery.as_ref().is_some_and(|rules| rules.interval == 0) {
            return Err("invalid recovery rules".into());
        }
        if let Some(owner) = &mut sites {
            owner.validate(&core)?;
            if let Some(maintenance) = &mut owner.maintenance {
                maintenance.enroll(&owner.facilities);
            }
        }
        if let Some(owner) = &mut construction {
            if owner.maintenance.is_some()
                || !owner.demolition.is_empty()
                || !owner.demolition_events.is_empty()
                || !owner.goal_builders.is_empty()
                || !owner.salvaging.is_empty()
                || !owner.salvage_events.is_empty()
            {
                return Err(
                    "demolition, salvage and goal-directed construction are not yet supported"
                        .into(),
                );
            }
            owner.normalize();
            let facilities = &sites
                .as_ref()
                .ok_or("construction requires work sites")?
                .facilities;
            let people: HashSet<_> = core.people.par_iter().map(|p| p.id).collect();
            let goods: HashSet<_> = core.physical.goods().par_iter().map(|g| g.id).collect();
            let view = core.physical.view(core.physical.tick())?;
            owner.validate(&people, &goods, facilities, &view, core.physical.tick())?;
        }
        if let Some(schedule) = &schedule {
            schedule.validate(&core)?;
        }
        let skill_rates = skills.as_ref().map(|owner| owner.owner.index());
        let skill_practice_recipes = skills
            .as_ref()
            .map_or_else(HashSet::new, CoreSkills::practice_recipes);
        Ok(Self {
            core,
            mechanics,
            autowork,
            tool_wear,
            tool_requirements,
            paused_work_rates,
            fatigue,
            activity,
            goal_setbacks,
            food_search,
            claims,
            social,
            personality,
            knowledge,
            health,
            medical,
            giving,
            practice_work_asks,
            practice_work_evidence,
            skills,
            taking,
            agriculture,
            irrigation,
            environment,
            recovery,
            purpose,
            gathering,
            fauna,
            hunting,
            animal_feelings,
            animal_care,
            animal_products,
            animal_restraints,
            animal_handling,
            family,
            dependent_care,
            skill_rates,
            skill_practice_recipes,
            personality_refusal_tick: None,
            personality_refusals: Vec::new(),
            work_checked_revision: None,
            work_checked_blocked: Vec::new(),
            opinions,
            goals,
            dreams,
            shared_work,
            market,
            writing,
            local_transport,
            construction,
            hauling,
            agreements,
            sites,
            schedule,
        })
    }
    /// Extract recorded opinion-change events for archival by the caller.
    /// Directed opinions and the rules used for future decisions are unchanged.
    pub fn take_opinion_events(&mut self) -> Vec<game_opinions::Event> {
        self.opinions
            .as_mut()
            .map_or_else(Vec::new, |owner| std::mem::take(&mut owner.events))
    }

    pub fn snapshot(&self) -> CoreHostSnapshot {
        CoreHostSnapshot {
            core: self.core.snapshot(),
            mechanics: self.mechanics.as_ref().map(mechanics::Runtime::snapshot),
            autowork: self.autowork.clone(),
            tool_wear: self.tool_wear.clone(),
            tool_requirements: self.tool_requirements.clone(),
            paused_work_rates: self.paused_work_rates.clone(),
            fatigue: self.fatigue.clone(),
            activity: self.activity.clone(),
            goal_setbacks: self.goal_setbacks.clone(),
            food_search: self.food_search.clone(),
            claims: self.claims.clone(),
            social: self.social.clone(),
            personality: self.personality.clone(),
            knowledge: self.knowledge.clone(),
            health: self.health.clone(),
            medical: self.medical.clone(),
            giving: self.giving.clone(),
            practice_work_asks: self.practice_work_asks.clone(),
            practice_work_evidence: self.practice_work_evidence.clone(),
            skills: self.skills.clone(),
            taking: self.taking.clone(),
            agriculture: self.agriculture.clone(),
            irrigation: self.irrigation.clone(),
            environment: self.environment.clone(),
            recovery: self.recovery.clone(),
            purpose: self.purpose.clone(),
            gathering: self.gathering.clone(),
            fauna: self.fauna.clone(),
            hunting: self.hunting.clone(),
            animal_feelings: self.animal_feelings.clone(),
            animal_care: self.animal_care.clone(),
            animal_products: self.animal_products.clone(),
            animal_restraints: self.animal_restraints.clone(),
            animal_handling: self.animal_handling.clone(),
            family: self.family.clone(),
            dependent_care: self.dependent_care.clone(),
            opinions: self.opinions.clone(),
            goals: self.goals.clone(),
            dreams: self.dreams.clone(),
            shared_work: self.shared_work.clone(),
            market: self.market.clone(),
            writing: self.writing.clone(),
            local_transport: self.local_transport.clone(),
            construction: self.construction.clone(),
            hauling: self.hauling.clone(),
            agreements: self.agreements.clone(),
            sites: self.sites.clone(),
            schedule: self.schedule.clone(),
        }
    }
    pub fn core(&self) -> &CoreInterval {
        &self.core
    }
    pub fn progress(&mut self, millis: u64) -> Result<u64, String> {
        if self.schedule.is_some() {
            return Err("use scheduled advancement while a schedule is active".into());
        }
        if self
            .core
            .elapsed_millis()
            .is_some_and(|offset| offset < self.core.physical.tick_millis())
        {
            self.sync_work_rates(&[])?;
        }
        self.progress_with_activity(millis, &[])
    }
    pub fn next_deadline(&mut self, hunger: Option<u32>) -> Result<CoreDeadline, String> {
        if self
            .core
            .elapsed_millis()
            .is_some_and(|offset| offset < self.core.physical.tick_millis())
        {
            self.sync_work_rates(&[])?;
        }
        self.core.next_deadline(hunger)
    }
    pub fn publish_arrivals(&mut self) -> Result<Option<CoreArrivals>, String> {
        self.core.publish_arrivals()
    }
    /// Start journeys selected by the enclosing decision owner.
    pub fn depart(
        &mut self,
        requests: &[travel::Departure],
        occupied: &[u64],
    ) -> Result<Vec<travel::DepartureOutcome>, String> {
        let tick = self.core.physical.tick();
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        let blocked = self.blocked_with_rest(occupied);
        let outcomes = self.core.depart(
            tick,
            offset,
            requests,
            blocked.as_deref().unwrap_or(occupied),
        )?;
        self.work_checked_revision = None;
        Ok(outcomes)
    }
    /// Let each eligible person pursue their own configured destination.
    pub fn depart_autonomously(
        &mut self,
        occupied: &[u64],
    ) -> Result<Vec<travel::DepartureOutcome>, String> {
        if occupied.par_windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err("invalid occupied people".into());
        }
        let Some(travel) = &self.core.travel else {
            return Ok(Vec::new());
        };
        if travel.destinations.is_empty() {
            return Ok(Vec::new());
        }
        let occupied_rest = self.blocked_with_rest(occupied);
        let occupied = occupied_rest.as_deref().unwrap_or(occupied);
        let mut busy = occupied.to_vec();
        busy.par_extend(
            self.core
                .physical
                .work()
                .par_iter()
                .filter(|row| row.phase == game_lot_runtime::Phase::Working)
                .map(|row| row.job.worker),
        );
        busy.par_sort_unstable();
        busy.dedup();
        let mut routes: Vec<_> = travel
            .routes
            .par_iter()
            .map(|r| (r.source, r.destination))
            .collect();
        routes.par_sort_unstable();
        let mut requests: Vec<_> = travel
            .destinations
            .par_iter()
            .filter_map(|choice| {
                if !choice.enabled || busy.binary_search(&choice.person).is_ok() {
                    return None;
                }
                let person = self
                    .core
                    .people
                    .binary_search_by_key(&choice.person, |p| p.id)
                    .ok()
                    .map(|i| &self.core.people[i])?;
                let source = person.region?;
                (person.health > 0
                    && !person.controlled
                    && routes.binary_search(&(source, choice.destination)).is_ok())
                .then_some(travel::Departure {
                    person: choice.person,
                    destination: choice.destination,
                })
            })
            .collect();
        requests.par_sort_unstable_by_key(|choice| choice.person);
        if requests.is_empty() {
            return Ok(Vec::new());
        }
        self.depart(&requests, occupied)
    }
    pub fn defer_work(&mut self, tick: u64, offset: u64, jobs: &[u64]) -> Result<(), String> {
        self.core.defer_work(tick, offset, jobs)
    }
    /// Admit actions selected by an enclosing owner, including work motivated
    /// by pressure rather than willingness. Physical eligibility still applies.
    pub fn start_chosen_work(
        &mut self,
        requests: &[(game_production_model::Start, u64)],
    ) -> Result<Vec<(u64, bool)>, String> {
        let helpers = self.assisting_people();
        if requests
            .par_iter()
            .any(|(start, _)| helpers.binary_search(&start.worker).is_ok())
        {
            return Err("an assisting person cannot start other work".into());
        }
        self.start_with_sites(requests)
    }
    /// A worker may independently stop work; inputs return at the boundary.
    pub fn abandon_work(&mut self, requests: &[(u64, u64)]) -> Result<(), String> {
        let tick = self.core.physical.tick();
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        let stopped: Vec<_> = requests.par_iter().map(|&(job, _)| (job, false)).collect();
        let next = self.stage_request_resolution(&stopped, tick, &self.core.people, false);
        let mut ended: Vec<_> = requests.par_iter().map(|&(job, _)| job).collect();
        ended.par_sort_unstable();
        let sites = self.stage_sites(&[], &ended, false);
        self.core.abandon_work(tick, offset, requests)?;
        self.finished_shared_jobs(&ended, &[]);
        self.prune_successions();
        self.record_early_work_outcomes(&[], &ended);
        self.paused_work_rates
            .retain(|row| ended.binary_search(&row.0).is_err());
        self.work_checked_revision = None;
        self.publish_request_resolution(next);
        if let Some(sites) = sites {
            self.sites = Some(sites);
        }
        Ok(())
    }
    /// Physical transfers do not imply any social consent or ownership rule.
    pub fn transfer_goods(
        &mut self,
        actions: &[game_lots::Transaction],
    ) -> Result<game_lot_runtime::PhysicalOutcomes, String> {
        let tick = self.core.physical.tick();
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        self.core.transfer_goods(tick, offset, actions)
    }

    fn stage_tools(
        &self,
        items: &[game_lot_runtime::ProducedItem],
        next: Option<u64>,
        updates: &[(u64, Option<game_lots::Place>)],
    ) -> Result<Option<ToolWear>, String> {
        let page_updates = self
            .writing
            .as_ref()
            .filter(|owner| {
                updates.par_iter().any(|(id, _)| {
                    owner
                        .pages
                        .binary_search_by_key(id, |page| page.item)
                        .is_ok()
                })
            })
            .map(|owner| {
                updates
                    .par_iter()
                    .filter(|(id, _)| {
                        owner
                            .pages
                            .binary_search_by_key(id, |page| page.item)
                            .is_err()
                    })
                    .copied()
                    .collect::<Vec<_>>()
            });
        let updates = page_updates.as_deref().unwrap_or(updates);
        // Mechanics tracks part condition by identity; custody changes stay in
        // the lot owner. A part also tracked as a tool still reaches that owner.
        let mechanical_updates = self
            .mechanics
            .as_ref()
            .filter(|mechanics| updates.par_iter().any(|(id, _)| mechanics.owns_part(*id)))
            .map(|mechanics| {
                updates
                    .par_iter()
                    .filter(|(id, _)| {
                        !mechanics.owns_part(*id)
                            || self.tool_wear.as_ref().is_some_and(|tools| {
                                tools
                                    .conditions
                                    .binary_search_by_key(id, |condition| condition.item)
                                    .is_ok()
                            })
                    })
                    .copied()
                    .collect::<Vec<_>>()
            });
        let updates = mechanical_updates.as_deref().unwrap_or(updates);
        let Some(owner) = &self.tool_wear else {
            if !updates.is_empty() || !items.is_empty() {
                return Err("identified changes require an item owner".into());
            }
            return Ok(None);
        };
        if items.is_empty() && updates.is_empty() && next.is_none_or(|id| id == owner.next_item) {
            return Ok(None);
        }
        if updates.par_iter().any(|(id, place)| {
            place.is_none()
                && owner
                    .conditions
                    .binary_search_by_key(id, |row| row.item)
                    .is_err()
        }) {
            return Err("identified removal has no tool owner".into());
        }
        Ok(Some(owner.stage_identified_outputs(
            items,
            next.unwrap_or(owner.next_item),
            updates,
        )?))
    }
    fn stage_written_page_removals(
        &self,
        updates: &[(u64, Option<game_lots::Place>)],
    ) -> Option<crate::Writing> {
        let owner = self.writing.as_ref()?;
        if !updates.par_iter().any(|(id, place)| {
            place.is_none()
                && owner
                    .pages
                    .binary_search_by_key(id, |page| page.item)
                    .is_ok()
        }) {
            return None;
        }
        let mut next = owner.clone();
        next.retire(updates);
        Some(next)
    }
    /// Publish completed nonperishable work and its staged tool conditions.
    pub fn publish_early_work(
        &mut self,
    ) -> Result<Option<game_lot_runtime::EarlyWorkEvents>, String> {
        let goods: Vec<_> = self.tool_wear.as_ref().map_or_else(Vec::new, |owner| {
            owner.rules.iter().map(|rule| rule.good).collect()
        });
        let config = self
            .tool_wear
            .as_ref()
            .map(|owner| game_lot_runtime::IdentifiedProduction {
                goods: &goods,
                next_item: owner.next_item,
            });
        let Some(draft) = self.core.prepare_early_work(config)? else {
            return Ok(None);
        };
        let purpose_progress = self.stage_purpose_progress(&draft.events().finished);
        let skill_completions = self.plan_skill_completions(&draft.events().finished);
        let tools = self.stage_tools(
            draft.produced_items(),
            draft.next_produced_item(),
            draft.item_updates(),
        )?;
        let writing = self.stage_written_page_removals(draft.item_updates());
        let mut outcomes: Vec<_> = draft
            .events()
            .finished
            .par_iter()
            .map(|&job| (job, true))
            .collect();
        outcomes.par_extend(
            draft
                .events()
                .interrupted
                .par_iter()
                .map(|&job| (job, false)),
        );
        let requests =
            self.stage_request_resolution(&outcomes, draft.events().tick, &self.core.people, false);
        let mut ended: Vec<_> = outcomes.par_iter().map(|&(job, _)| job).collect();
        ended.par_sort_unstable();
        let sites = self.stage_sites(&draft.events().finished, &ended, false);
        let goals = self.stage_goals(
            &draft.events().finished,
            draft.events().tick,
            &self.core.people,
            false,
        )?;
        let dreams = self.stage_aspiration_observations(
            requests.as_ref(),
            &self.core.people,
            draft.events().tick,
        );

        self.core.validate_early_work(&draft)?;
        let events = self.core.commit_early_work(draft)?;
        self.finished_shared_jobs(&ended, &events.finished);
        self.prune_successions();
        self.remember_skill_completions(skill_completions);
        self.record_early_work_outcomes(&events.finished, &events.interrupted);
        if let Some(progress) = purpose_progress {
            self.purpose.as_mut().expect("purpose owner").progress = progress;
        }
        if let Some(tools) = tools {
            self.tool_wear = Some(tools);
        }
        if let Some(writing) = writing {
            self.writing = Some(writing);
        }
        self.publish_request_resolution(requests);
        if let Some(dreams) = dreams {
            self.dreams = Some(dreams);
        }
        if let Some(sites) = sites {
            self.sites = Some(sites);
        }
        if let Some(goals) = goals {
            self.goals = Some(goals);
        }
        Ok(Some(events))
    }
    /// Stage and commit carried meals selected by individuals. Active workers
    /// and the host-provided occupied people are excluded.
    pub fn eat_autonomously(
        &mut self,
        blocked: &[u64],
    ) -> Result<Vec<game_lot_diet::Meal>, String> {
        if blocked.par_windows(2).any(|rows| rows[0] >= rows[1]) {
            return Err("invalid occupied people".into());
        }
        let occupied = self.blocked_with_rest(blocked);
        let mut busy = occupied.as_deref().unwrap_or(blocked).to_vec();
        busy.par_extend(
            self.core
                .physical
                .work()
                .par_iter()
                .filter(|row| row.phase == game_lot_runtime::Phase::Working)
                .map(|row| row.job.worker),
        );
        busy.par_sort_unstable();
        busy.dedup();
        let Some((meals, draft)) = self.core.prepare_autonomous_meals(&busy)? else {
            return Ok(Vec::new());
        };
        let tools = self.stage_tools(&[], None, draft.item_updates())?;
        let writing = self.stage_written_page_removals(draft.item_updates());
        self.core.validate_meals(&draft)?;
        self.core.commit_meals(draft)?;
        if let Some(tools) = tools {
            self.tool_wear = Some(tools);
        }
        if let Some(writing) = writing {
            self.writing = Some(writing);
        }
        Ok(meals)
    }
    /// Commit only the meals already selected by individuals.
    fn eat_selected(&mut self, meals: &[game_lot_diet::Meal]) -> Result<(), String> {
        let tick = self.core.physical.tick();
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        let Some(draft) = self.core.prepare_meals(tick, offset, meals)? else {
            return Ok(());
        };
        let tools = self.stage_tools(&[], None, draft.item_updates())?;
        let writing = self.stage_written_page_removals(draft.item_updates());
        self.core.validate_meals(&draft)?;
        self.core.commit_meals(draft)?;
        if let Some(tools) = tools {
            self.tool_wear = Some(tools);
        }
        if let Some(writing) = writing {
            self.writing = Some(writing);
        }
        Ok(())
    }
    /// Inspect individual work opportunities before shared-stock admission.
    /// Site, tool and physical resource admission can still deny an attempt.
    pub fn preview_autonomous_work(
        &self,
        blocked: &[u64],
        rate: u64,
    ) -> Result<Vec<game_autowork::Opportunity>, String> {
        if rate == 0 || blocked.par_windows(2).any(|rows| rows[0] >= rows[1]) {
            return Err("invalid occupied people or work rate".into());
        }
        let Some(owner) = &self.autowork else {
            return Ok(Vec::new());
        };
        let rest = self.blocked_with_rest(blocked);
        let mut busy = rest.as_deref().unwrap_or(blocked).to_vec();
        let tick = self.core.physical.tick();
        if self.personality_refusal_tick == Some(tick) {
            busy.par_extend(self.personality_refusals.par_iter().copied());
        } else if let Some(personality) = &self.personality {
            busy.par_extend(personality.work_refusals(self.core.seed, tick));
        }
        if let Some(family) = &self.family {
            busy.par_extend(game_economy_work::underage(family));
        }
        busy.par_sort_unstable();
        busy.dedup();
        let effective = self.effective_autonomous_work(&busy)?;
        let effective = effective.policy.as_ref().unwrap_or(owner);
        let filtered = self.claim_aware_work_policies(effective);
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        self.core.autonomous_work_opportunities(
            tick,
            offset,
            filtered.as_ref().unwrap_or(effective),
            &busy,
            rate,
        )
    }

    /// Admit personally chosen work. Other owners' activity, claim, site and
    /// tool restrictions must be supplied in `blocked` by the enclosing host.
    pub fn start_autonomous_work(
        &mut self,
        blocked: &[u64],
        rate: u64,
    ) -> Result<Vec<(u64, bool)>, String> {
        if blocked.par_windows(2).any(|rows| rows[0] >= rows[1]) {
            return Err("invalid occupied people".into());
        }
        let occupied = self.blocked_with_rest(blocked);
        let blocked = occupied.as_deref().unwrap_or(blocked);
        self.refresh_personality_work_refusals();
        let reluctant = (!self.personality_refusals.is_empty()).then(|| {
            let mut rows = blocked.to_vec();
            rows.par_extend(self.personality_refusals.par_iter().copied());
            rows.par_sort_unstable();
            rows.dedup();
            rows
        });
        let blocked = reluctant.as_deref().unwrap_or(blocked);
        let mut effective = self.effective_autonomous_work(blocked)?;
        let outcomes =
            self.start_autonomous_work_inner(blocked, rate, effective.policy.as_mut())?;
        self.record_goal_setbacks(&effective.goal_candidates, &outcomes);
        Ok(outcomes)
    }

    /// Prepare the same goal-derived policy for inspection and admission.
    fn effective_autonomous_work(&self, blocked: &[u64]) -> Result<EffectiveWork, String> {
        let mut effective = self
            .goals
            .as_ref()
            .zip(self.autowork.as_ref())
            .map(|(goals, owner)| goals.policies(owner, self.core.physical.tick()));
        let candidates = self.goal_work_candidates(blocked, effective.as_ref());
        if let (Some(goals), Some(base), Some(planned)) = (
            self.goals.as_ref().filter(|goals| goals.plan_inputs),
            self.autowork.as_ref(),
            effective.as_mut(),
        ) {
            let tick = self.core.physical.tick();
            let view = self.core.physical.view(tick)?;
            let production = game_lot_projection::production(
                &self.core.physical,
                &view,
                self.core.rules.max_work_hunger,
            );
            let people: Vec<_> = self
                .core
                .people
                .par_iter()
                .map(|person| (person.id, person.region, person.health, person.hunger))
                .collect();
            game_goals::planning::plan_at(
                planned,
                base,
                &production,
                &people,
                tick / goals.retry_interval,
            );
        }
        Ok(EffectiveWork {
            policy: effective,
            goal_candidates: candidates,
        })
    }
    fn start_autonomous_work_inner(
        &mut self,
        blocked: &[u64],
        rate: u64,
        effective: Option<&mut Autowork>,
    ) -> Result<Vec<(u64, bool)>, String> {
        if blocked.par_windows(2).any(|rows| rows[0] >= rows[1]) {
            return Err("invalid occupied people".into());
        }
        let occupied = self.blocked_with_rest(blocked);
        let blocked = occupied.as_deref().unwrap_or(blocked);
        let underage = self.family.as_ref().map(game_economy_work::underage);
        let unavailable = underage.filter(|rows| !rows.is_empty()).map(|mut rows| {
            rows.par_extend(blocked.par_iter().copied());
            rows.par_sort_unstable();
            rows.dedup();
            rows
        });
        let blocked = unavailable.as_deref().unwrap_or(blocked);
        if let Some(result) = self.start_claim_aware_work(blocked, rate, effective.as_deref())? {
            return Ok(result);
        }
        if self.sites.is_some() || !self.tool_requirements.is_empty() || self.skills.is_some() {
            let Some(owner) = &self.autowork else {
                return Ok(Vec::new());
            };
            let tick = self.core.physical.tick();
            let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
            let Some(plan) = self.core.plan_autonomous_work(
                tick,
                offset,
                effective.as_deref().unwrap_or(owner),
                blocked,
                rate,
            )?
            else {
                return Ok(Vec::new());
            };
            let requests: Vec<_> = plan
                .requests
                .par_iter()
                .map(|start| (*start, rate))
                .collect();
            let outcomes = self.start_with_sites(&requests)?;
            self.autowork.as_mut().expect("checked work owner").next_id = plan.next_id;
            return Ok(outcomes);
        }
        let Some(owner) = &mut self.autowork else {
            return Ok(Vec::new());
        };
        let tick = self.core.physical.tick();
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        if let Some(effective) = effective {
            let result = self
                .core
                .start_autonomous_work(tick, offset, effective, blocked, rate)?;
            owner.next_id = effective.next_id;
            Ok(result)
        } else {
            self.core
                .start_autonomous_work(tick, offset, owner, blocked, rate)
        }
    }
    /// Settle the completed quantum after staging tool conditions.
    pub fn settle(&mut self) -> Result<Events, String> {
        self.settle_with_occupied(&[])
    }
    /// Settle while excluding people occupied by owners outside this host.
    pub fn settle_with_occupied(&mut self, occupied: &[u64]) -> Result<Events, String> {
        if occupied.par_windows(2).any(|rows| rows[0] >= rows[1]) {
            return Err("invalid occupied people".into());
        }
        let external = occupied;
        let mut dependent = self.plan_dependent_care(occupied)?;
        let mut actions = dependent
            .as_mut()
            .map_or_else(Vec::new, |plan| std::mem::take(&mut plan.actions));
        let active = dependent.as_ref().map(|plan| {
            let mut active = occupied.to_vec();
            active.par_extend(plan.actors.par_iter().copied());
            active.par_extend(game_economy_work::care::dependents(
                self.dependent_care.as_ref().expect("care owner"),
                self.family.as_ref().expect("care family"),
            ));
            active.par_sort_unstable();
            active.dedup();
            active
        });
        let occupied = active.as_deref().unwrap_or(occupied);
        let dependents = if self.medical.is_some() || self.giving.is_some() {
            self.dependent_care
                .as_ref()
                .zip(self.family.as_ref())
                .map_or_else(Vec::new, |(care, family)| {
                    game_economy_work::care::dependents(care, family)
                })
        } else {
            Vec::new()
        };
        let medical_base = u64::try_from(actions.len())
            .map_err(|_| "medical action overflow")?
            .checked_add(1)
            .ok_or("medical action overflow")?;
        let mut medical = self.plan_medical(external, occupied, &dependents, medical_base)?;
        let medical_busy = medical
            .as_mut()
            .filter(|plan| !plan.actors.is_empty())
            .map(|plan| {
                let mut busy = occupied.to_vec();
                busy.par_extend(plan.actors.par_iter().copied());
                busy.par_sort_unstable();
                busy.dedup();
                actions.par_extend(std::mem::take(&mut plan.actions).into_par_iter());
                busy
            });
        let occupied = medical_busy.as_deref().unwrap_or(occupied);
        let giving_base = u64::try_from(actions.len())
            .map_err(|_| "sharing action overflow")?
            .checked_add(1)
            .ok_or("sharing action overflow")?;
        let mut giving = self.plan_giving(
            external,
            occupied,
            &dependents,
            medical.as_ref(),
            giving_base,
        )?;
        let giving_busy = giving
            .as_mut()
            .filter(|frame| !frame.actors.is_empty())
            .map(|frame| {
                let mut busy = occupied.to_vec();
                busy.par_extend(frame.actors.par_iter().copied());
                busy.par_sort_unstable();
                busy.dedup();
                actions.par_extend(std::mem::take(&mut frame.plan.actions).into_par_iter());
                busy
            });
        let occupied = giving_busy.as_deref().unwrap_or(occupied);
        let taking_base = u64::try_from(actions.len())
            .map_err(|_| "taking action overflow")?
            .checked_add(1)
            .ok_or("taking action overflow")?;
        let mut taking = self.plan_taking(occupied, taking_base)?;
        let taking_busy = taking.as_mut().map(|frame| {
            let mut busy = occupied.to_vec();
            busy.par_extend(frame.plan.actors.par_iter().copied());
            busy.par_sort_unstable();
            busy.dedup();
            actions.par_extend(frame.plan.take_actions().into_par_iter().map(|row| {
                game_lots::Transaction {
                    id: row.action,
                    movements: vec![row.movement],
                    creations: vec![],
                }
            }));
            busy
        });
        let occupied = taking_busy.as_deref().unwrap_or(occupied);
        let base = u64::try_from(actions.len())
            .map_err(|_| "care action overflow")?
            .checked_add(1)
            .ok_or("care action overflow")?;
        let mut cultivation = self.plan_cultivation(occupied, base)?;
        let compact = game_economy_agriculture::append_planting(&mut cultivation, &mut actions);
        let irrigation_plan = self.plan_irrigation(occupied, &mut cultivation, &mut actions)?;
        let gathering_base = if self.gathering.is_some() {
            let changes = cultivation
                .as_ref()
                .map_or(&[][..], |plan| plan.changes.as_slice());
            actions
                .par_iter()
                .map(|action| action.id)
                .chain(changes.par_iter().map(|row| row.1))
                .max()
                .map_or(Ok(1), |id| {
                    id.checked_add(1).ok_or("gathering action overflow")
                })?
        } else {
            1
        };
        let gathering = self.plan_gathering(
            occupied,
            cultivation.as_ref(),
            irrigation_plan.as_ref(),
            gathering_base,
        )?;
        let goods: Vec<_> = self.tool_wear.as_ref().map_or_else(Vec::new, |owner| {
            owner.rules.iter().map(|rule| rule.good).collect()
        });
        let config = self
            .tool_wear
            .as_ref()
            .map(|owner| game_lot_runtime::IdentifiedProduction {
                goods: &goods,
                next_item: owner.next_item,
            });
        let mut growth = game_economy_agriculture::physical_growth(cultivation.as_ref(), &compact);
        growth.gathered = gathering
            .as_ref()
            .map_or(&[], |(_, outputs)| outputs.as_slice());
        let (care, restraints, handling, hunting) = if self.hunting.is_some()
            || self.animal_care.is_some()
            || self.animal_handling.is_some()
            || self.animal_restraints.is_some()
        {
            let mut next_action = if self.hunting.is_some() || self.animal_care.is_some() {
                actions
                    .par_iter()
                    .map(|row| row.id)
                    .chain(growth.harvest.par_iter().map(|row| row.id))
                    .chain(growth.gathered.par_iter().map(|row| row.action))
                    .max()
                    .map_or(Ok(1), |id| {
                        id.checked_add(1).ok_or("animal action overflow")
                    })?
            } else {
                1
            };
            let mut care = self.plan_animal_care(
                occupied,
                cultivation.as_ref(),
                irrigation_plan.as_ref(),
                gathering.as_ref().map(|(plan, _)| plan),
                next_action,
            )?;
            if let Some(plan) = &mut care {
                next_action = next_action
                    .checked_add(plan.actions.len() as u64)
                    .ok_or("animal action overflow")?;
                actions.par_extend(std::mem::take(&mut plan.actions).into_par_iter());
            }
            let restraints = self.plan_animal_restraints(
                occupied,
                (
                    cultivation.as_ref(),
                    irrigation_plan.as_ref(),
                    gathering.as_ref().map(|(plan, _)| plan),
                    care.as_ref(),
                ),
            )?;
            let handling = self.plan_animal_handling(
                occupied,
                cultivation.as_ref(),
                irrigation_plan.as_ref(),
                gathering.as_ref().map(|(plan, _)| plan),
                care.as_ref(),
                restraints.as_ref(),
            )?;
            let hunting = self.plan_hunting(
                occupied,
                cultivation.as_ref(),
                irrigation_plan.as_ref(),
                gathering.as_ref().map(|(plan, _)| plan),
                (care.as_ref(), restraints.as_ref(), handling.as_ref()),
                next_action,
            )?;
            (care, restraints, handling, hunting)
        } else {
            (None, None, None, None)
        };
        growth.animal_yield = hunting.as_ref().map_or(&[], |plan| plan.yields.as_slice());
        let products = if self.animal_products.is_some() {
            let product_base = actions
                .par_iter()
                .map(|row| row.id)
                .chain(growth.harvest.par_iter().map(|row| row.id))
                .chain(growth.gathered.par_iter().map(|row| row.action))
                .chain(growth.animal_yield.par_iter().map(|row| row.action))
                .max()
                .map_or(Ok(1), |id| {
                    id.checked_add(1).ok_or("animal product action overflow")
                })?;
            self.plan_animal_products(
                occupied,
                (
                    cultivation.as_ref(),
                    irrigation_plan.as_ref(),
                    gathering.as_ref().map(|(plan, _)| plan),
                    care.as_ref(),
                    restraints.as_ref(),
                    handling.as_ref(),
                    hunting.as_ref(),
                ),
                product_base,
            )?
        } else {
            None
        };
        growth.animal_products = products.as_ref().map_or(&[], |plan| plan.yields.as_slice());
        let mut draft = self
            .core
            .prepare_settlement_with_growth(&actions, growth, config)?;
        let tick = draft.events().tick;
        let finished_for_purpose = self
            .purpose
            .as_ref()
            .map(|_| draft.events().finished.clone());
        let mut purpose = finished_for_purpose
            .as_ref()
            .and_then(|finished| self.stage_purpose(finished, &mut draft.people.people, tick));
        let fatigue = self.stage_fatigue(&mut draft.people.people)?;
        if self
            .environment
            .as_ref()
            .is_some_and(|owner| !owner.regions.is_empty())
        {
            let tick = draft.events().tick;
            let view = self.core.physical.view(tick)?;
            crate::environment::apply(
                self.environment.as_ref(),
                &view,
                self.core.travel.as_ref(),
                &mut draft.people.people,
                tick,
            );
        }
        if let Some(rules) = self
            .recovery
            .as_ref()
            .filter(|rules| draft.events().tick.is_multiple_of(rules.interval))
        {
            let quantum = self.core.physical.tick_millis();
            draft.people.people.par_iter_mut().for_each(|person| {
                if person.health == 0 || person.hunger > rules.max_hunger {
                    return;
                }
                let rest = self
                    .activity
                    .binary_search_by_key(&person.id, |row| row.id)
                    .ok()
                    .map_or(0, |index| self.activity[index].rest_millis);
                let bonus = ((u128::from(rules.rest_bonus) * u128::from(rest))
                    / u128::from(quantum)) as u32;
                person.health = person
                    .health
                    .saturating_add(rules.health_per_interval.saturating_add(bonus))
                    .min(self.core.rules.max_health);
            });
        }
        if let Some(plan) = &dependent {
            plan.apply_health(
                self.dependent_care
                    .as_ref()
                    .expect("care owner")
                    .rules
                    .neglect_damage,
                &mut draft.people.people,
            );
        }
        let skills = self.stage_skills(&draft.events().finished, draft.events().tick);
        let family = self.stage_family(&mut draft)?;
        if family.is_none() {
            let mut deceased: Vec<_> = self
                .core
                .physical
                .work()
                .par_iter()
                .filter(|work| work.phase == game_lot_runtime::Phase::Working)
                .filter_map(|work| {
                    let id = work.job.worker;
                    let old = self
                        .core
                        .people
                        .binary_search_by_key(&id, |person| person.id)
                        .ok()?;
                    let current = draft
                        .people
                        .people
                        .binary_search_by_key(&id, |person| person.id)
                        .ok()?;
                    (self.core.people[old].health > 0 && draft.people.people[current].health == 0)
                        .then_some(id)
                })
                .collect();
            deceased.par_sort_unstable();
            deceased.dedup();
            if !deceased.is_empty() {
                let (eligible, until) =
                    self.succession_death_suspensions(&deceased, draft.events().tick)?;
                self.core.physical.stage_interval_people_with_suspension(
                    &mut draft.physical,
                    &[],
                    &deceased,
                    &eligible,
                    until,
                )?;
            }
        }
        let personality = self.stage_personality_births(family.as_ref());
        let skills = self.stage_skill_births(skills, family.as_ref(), draft.events().tick)?;
        crate::purpose::enroll(
            purpose.as_mut().map(|owner| &mut owner.owner),
            family.as_ref().map(|frame| &frame.owner),
            draft.events().tick,
        )?;
        let medical_resolved = medical.as_ref().map(|plan| plan.resolve(draft.events()));
        let birth_health = self.stage_birth_health(family.as_ref(), draft.events().tick);
        let birth_health =
            self.stage_care_health(birth_health, dependent.as_ref(), draft.events().tick)?;
        let giving_help = giving
            .as_ref()
            .map_or_else(Vec::new, |frame| frame.plan.help(draft.events()));
        let mut contacts = medical_resolved
            .as_ref()
            .map_or_else(Vec::new, |resolved| resolved.contacts());
        contacts.par_extend(giving_help.par_iter().copied());
        if let Some(frame) = &giving {
            contacts.par_extend(frame.contacts.par_iter().copied());
        }
        let mut taking_changes = if let Some(frame) = &mut taking {
            frame.owner.events = frame.plan.publish(draft.events(), &[]);
            contacts.par_extend(
                frame
                    .owner
                    .events
                    .par_iter()
                    .filter(|event| event.outcome == game_economy_taking::Outcome::Transferred)
                    .map(|event| (event.actor, event.target)),
            );
            let mut changes = self.stage_taking_reactions(&mut frame.owner);
            changes.par_extend(self.stage_taking_contacts(
                &mut frame.owner,
                &contacts,
                draft.events().tick,
            ));
            changes
        } else {
            Vec::new()
        };
        let birth_health =
            self.stage_contact_health(birth_health, &contacts, draft.events().tick)?;
        let birth_health =
            self.stage_medical_health(birth_health, medical_resolved.as_ref(), draft.events().tick);
        if let Some(frame) = &mut giving {
            taking_changes.par_extend(self.stage_practices(
                frame,
                &contacts,
                draft.events(),
                draft.events().tick,
            )?);
            frame.plan.learn_sharing(
                &mut frame.owner,
                draft.events(),
                self.core.physical.goods(),
                draft.events().tick,
            );
            frame.plan.clear_fulfilled_requests(
                &mut frame.owner,
                draft.events(),
                self.core.physical.goods(),
            );
        }
        let irrigation = self
            .irrigation
            .as_ref()
            .zip(irrigation_plan)
            .map(|(owner, plan)| owner.advance(plan, draft.events(), draft.events().tick));
        let care_uses = dependent
            .as_ref()
            .map_or(&[][..], |plan| plan.uses.as_slice());
        let (agriculture, mut claims) =
            self.stage_cultivation(cultivation, draft.events(), care_uses)?;
        let mut medical_opinions = None;
        let mut help = medical_resolved
            .as_ref()
            .map_or_else(Vec::new, |resolved| resolved.help());
        help.par_extend(giving_help.into_par_iter());
        if !help.is_empty() {
            if let Some((_, opinions)) = &mut claims {
                if let Some(opinions) = opinions.as_mut().filter(|owner| owner.tracks_help()) {
                    opinions.learn_help(&help);
                }
            } else if let Some(mut opinions) =
                self.opinions.clone().filter(|owner| owner.tracks_help())
            {
                opinions.learn_help(&help);
                medical_opinions = Some(opinions);
            }
        }
        if !taking_changes.is_empty() {
            if let Some((_, Some(opinions))) = &mut claims {
                opinions.apply_personal_changes(&taking_changes, draft.events().tick);
            } else if let Some(opinions) = medical_opinions.as_mut() {
                opinions.apply_personal_changes(&taking_changes, draft.events().tick);
            } else if let Some(mut opinions) = self.opinions.clone() {
                opinions.apply_personal_changes(&taking_changes, draft.events().tick);
                medical_opinions = Some(opinions);
            }
        }
        let mut gathering = gathering
            .map(|(plan, _)| {
                plan.publish(
                    self.gathering.as_ref().expect("gathering owner"),
                    draft.events(),
                    self.environment
                        .as_ref()
                        .and_then(|owner| owner.season(draft.events().tick)),
                )
            })
            .transpose()?;
        let hunted = hunting
            .as_ref()
            .map_or_else(Vec::new, |plan| plan.accepted(draft.events()));
        let feeding = care
            .as_ref()
            .map_or_else(Vec::new, |plan| plan.accepted(draft.events()));
        let (products, product_handling) = products.map_or((None, Vec::new()), |plan| {
            let (events, handling) = plan.publish(draft.events());
            let owner = self.animal_products.as_ref().expect("product owner");
            let fauna = self.fauna.as_ref().expect("product fauna");
            (Some(owner.advance(fauna, events)), handling)
        });
        let animal_feelings = self.stage_animal_feelings(
            hunting.as_ref(),
            &hunted,
            &feeding,
            handling.as_ref(),
            &product_handling,
            restraints.as_ref(),
        );
        let fauna = self.stage_fauna(&mut gathering, &hunted, &feeding, draft.events().tick)?;
        let (mut next_restraints, restraint_reactions) =
            restraints.map_or((None, Vec::new()), |plan| (plan.state, plan.reactions));
        if let (Some(owner), Some(fauna)) = (&mut next_restraints, &fauna) {
            let available = |_person| true;
            match fauna {
                fauna::CoreFaunaDraft::Quiet(lifecycle) => owner.reconcile_with_deaths(
                    &draft.people.people,
                    self.fauna.as_ref().expect("staged fauna"),
                    &available,
                    draft.events().tick,
                    lifecycle.deaths(),
                ),
                fauna::CoreFaunaDraft::Complete(fauna) => {
                    owner.reconcile(&draft.people.people, fauna, &available, draft.events().tick)
                }
            }
        }
        let mut tools = self.stage_tools(
            draft.produced_items(),
            draft.next_produced_item(),
            draft.item_updates(),
        )?;
        let writing = self.stage_written_page_removals(draft.item_updates());
        if tools.is_none() {
            tools = self
                .tool_wear
                .as_ref()
                .filter(|owner| !owner.events.is_empty())
                .cloned();
        }
        if let Some(tools) = &mut tools {
            tools.events.clear();
        }
        let mut outcomes: Vec<_> = draft
            .events()
            .finished
            .par_iter()
            .map(|&job| (job, true))
            .collect();
        outcomes.par_extend(
            draft
                .events()
                .interrupted
                .par_iter()
                .chain(draft.events().abandoned.par_iter())
                .chain(draft.events().suspended.par_iter())
                .map(|&job| (job, false)),
        );
        outcomes.par_sort_unstable_by_key(|&(job, success)| (job, !success));
        outcomes.dedup_by_key(|row| row.0);
        let requests = self.stage_request_resolution(
            &outcomes,
            draft.events().tick,
            &draft.people.people,
            true,
        );
        let mut goals = self.stage_goals(
            &draft.events().finished,
            draft.events().tick,
            &draft.people.people,
            true,
        )?;
        let dreams = self.stage_aspirations(
            &mut goals,
            requests.as_ref(),
            &draft.people.people,
            family.as_ref(),
            draft.events().tick,
        )?;
        let mut interrupted: Vec<_> = draft
            .events()
            .interrupted
            .par_iter()
            .chain(draft.events().suspended.par_iter())
            .chain(draft.events().abandoned.par_iter())
            .copied()
            .collect();
        interrupted.par_sort_unstable();
        let shared_work = self.stage_shared_boundary(&draft.events().finished, &interrupted);
        let market = self.stage_market_boundary();
        let mut ended = draft.events().finished.clone();
        ended.par_extend(draft.events().interrupted.par_iter().copied());
        ended.par_extend(draft.events().abandoned.par_iter().copied());
        ended.par_extend(self.core.physical.work().par_iter().filter(|work| matches!(work.phase, game_lot_runtime::Phase::Suspended { until } if until <= draft.events().tick)).map(|work| work.job.id));
        let sites = self.stage_sites(&draft.events().finished, &ended, true);
        let agreements = self.agreements.as_ref().and_then(|owner| {
            (!owner.events.is_empty()
                || owner.contracts.par_iter().any(|contract| {
                    contract.due < draft.events().tick
                        && matches!(
                            contract.status,
                            game_agreements::Status::Active | game_agreements::Status::Proposed
                        )
                }))
            .then(|| owner.advance(draft.events().tick))
        });
        self.core.validate_settlement(&draft)?;
        let events = self.core.commit_settlement(draft)?;
        if let Some(owner) = &mut self.local_transport {
            owner.events.clear();
        }
        if let Some(owner) = &mut self.construction {
            owner.events.clear();
        }
        self.goal_setbacks.clear();
        self.practice_work_asks.clear();
        self.practice_work_evidence = CorePracticeWorkEvidence::default();
        if let Some(owner) = personality {
            self.personality = Some(owner);
        }
        if let Some(owner) = skills {
            if !owner.owner.events.is_empty() {
                self.skill_rates = Some(owner.owner.index());
            }
            self.skills = Some(owner);
        }
        if let Some(owner) = purpose {
            self.purpose = Some(owner);
        }
        if let Some(tools) = tools {
            self.tool_wear = Some(tools);
        }
        if let Some(writing) = writing {
            self.writing = Some(writing);
        }
        self.publish_request_resolution(requests);
        if let Some(owner) = shared_work {
            self.shared_work = Some(owner);
        }
        if let Some(owner) = market {
            self.market = Some(owner);
        }
        self.prune_successions();
        if let Some(dreams) = dreams {
            self.dreams = Some(dreams);
        }
        if let Some(sites) = sites {
            self.sites = Some(sites);
        }
        if let Some(goals) = goals {
            self.goals = Some(goals);
        }
        if let Some(agreements) = agreements {
            self.agreements = Some(agreements);
        }
        if let Some(owner) = agriculture {
            self.agriculture = Some(owner);
        }
        if let Some(owner) = irrigation {
            self.irrigation = Some(owner);
        }
        if let Some(owner) = gathering {
            self.gathering = Some(owner);
        }
        let newborns = if let Some(family) = family {
            if let Some(care) = &mut self.dependent_care {
                game_economy_work::care::inherit(care, &family.owner, &family.newborns);
            }
            self.family = Some(family.owner);
            family.newborns
        } else {
            Vec::new()
        };
        if let Some(health) = birth_health {
            self.health = Some(health);
        }
        if let Some(frame) = giving {
            self.giving = Some(frame.owner);
        }
        if let Some(frame) = taking {
            self.taking = Some(frame.owner);
        }
        if let Some(plan) = dependent {
            plan.publish(self.dependent_care.as_mut().expect("care owner"));
        }
        self.publish_fauna(fauna);
        if !restraint_reactions.is_empty() {
            game_fauna::restraint::apply_reactions(
                self.fauna.as_mut().expect("restraint fauna"),
                &restraint_reactions,
            );
        }
        if let Some(owner) = next_restraints {
            self.animal_restraints = Some(owner);
        }
        if let Some(plan) = handling {
            game_fauna::handling::apply(self.fauna.as_mut().expect("handling fauna"), &plan.events);
            self.animal_handling
                .as_mut()
                .expect("handling owner")
                .events = plan.events;
        }
        if let Some(owner) = products {
            self.animal_products = Some(owner);
        }
        if !product_handling.is_empty() {
            game_fauna::handling::apply(
                self.fauna.as_mut().expect("product fauna"),
                &product_handling,
            );
        }
        if let Some(feelings) = animal_feelings {
            self.animal_feelings = Some(feelings);
        }
        if let Some((owner, opinions)) = claims {
            self.claims = Some(owner);
            self.opinions = opinions;
        } else if let Some(opinions) = medical_opinions {
            self.opinions = Some(opinions);
        }
        self.inherit_birth_preferences(&newborns, events.tick);
        if let Some(fatigue) = fatigue {
            self.fatigue = Some(fatigue);
            self.activity = self
                .core
                .people
                .par_iter()
                .map(|person| game_economy_fatigue::TimedActivity {
                    id: person.id,
                    ..Default::default()
                })
                .collect();
        }
        self.paused_work_rates.retain(|(job, _)| {
            self.core
                .physical
                .work()
                .binary_search_by_key(job, |row| row.job.id)
                .ok()
                .is_some_and(|index| {
                    self.core.physical.work()[index].phase == game_lot_runtime::Phase::Working
                })
        });
        self.work_checked_revision = None;
        self.schedule = None;
        Ok(events)
    }
}
