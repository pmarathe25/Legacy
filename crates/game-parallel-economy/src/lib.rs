//! Person-driven economic activity over authoritative dated physical goods.
pub use game_parallel_agreements::OfferPolicy as PromiseOfferPolicy;
pub use game_parallel_economy_building::demolition::{
    Event as DemolitionEvent, Policy as DemolitionPolicy,
};
pub use game_parallel_economy_building::{
    Blueprint as BuildingBlueprint, Construction, Event as ConstructionEvent, GoalBuilder,
    Policy as ConstructionPolicy,
};
pub use game_parallel_economy_campaign::Campaign;
pub use game_parallel_economy_combat::Combat;
pub use game_parallel_economy_dynamics::Dynamics;
pub use game_parallel_economy_expeditions::{MissionPhase, Missions, Start as MissionStart};
use game_parallel_economy_family as family;
pub use game_parallel_economy_family::Families;
pub use game_parallel_economy_fatigue::Fatigue;
pub use game_parallel_economy_goals::{Dream, Dreams, Goals};
pub use game_parallel_economy_knowledge::Knowledge;
use game_parallel_economy_person as person;
pub use game_parallel_economy_person::travel::{Colocation, Departure, Journey, Route, Travel};
pub use game_parallel_economy_promises::{
    Decision as PromiseDecision, Proposal as PromiseProposal,
};
pub use game_parallel_economy_skills::Skills;
pub use game_parallel_economy_work::care::DependentCare;
pub use game_parallel_economy_work::sites::{
    Binding as WorkSiteBinding, Facilities as WorkSites, Requirement as WorkSiteRequirement,
    Site as WorkSite,
};
pub use game_parallel_economy_work::tools::Requirement as WorkTool;
pub use game_parallel_opinions::{
    ObligationDevelopment, Opinion, RelationshipCase, Sensitivity as OpinionSensitivity,
};
mod construction_planning;
mod development;
mod environment;
mod food_search;
mod gathering;
mod giving;
mod giving_discovery;
mod giving_inheritance;
mod hauling;
mod medical;
pub use food_search::{FoodSearch, Preference as FoodSearchPreference};
pub use giving::{
    Asking as FoodAsking, Giving, HelperAsking, Policy as GivingPolicy, Request as FoodRequest,
    Response as FoodResponse, SharingLearning,
};
pub use giving_discovery::Policy as AcquaintanceAsking;
pub use hauling::{
    Asking as HaulingAsking, Hauling, Policy as HaulingPolicy, Request as HaulingRequest,
};
pub use medical::{Medical, Policy as MedicalPolicy};
mod purpose;
mod recovery;
pub use game_parallel_purpose::Purpose;
pub use gathering::{Gathering, Node as ResourceNode, Policy as GatherPolicy};
pub use recovery::Rules as RecoveryRules;
mod state;
use game_parallel_autowork::Autowork;
pub use game_parallel_economy_agriculture::{
    Agriculture, Crop, Field, Phase as CropPhase, Policy as FarmPolicy,
};
pub use game_parallel_environment::{Environment, Hazard, Season};
use game_parallel_lot_projection::agreement;
use game_parallel_lot_runtime::{AdvanceDraft, Events, Runtime};
use game_parallel_lots::Transaction;
use game_parallel_market::Market;
use game_parallel_nutrition::Nutrition;
use game_parallel_opinions::Opinions;
use game_parallel_production_model::FoodEffect;
use rayon::prelude::*;
pub use state::{Agreements, Person, Rules, State};
use std::collections::HashSet;
pub struct Economy {
    physical: Runtime,
    seed: u64,
    people: Vec<Person>,
    rules: Rules,
    market: Option<Market>,
    agreements: Option<Agreements>,
    opinions: Option<Opinions>,
    autowork: Option<Autowork>,
    work_tools: Vec<WorkTool>,
    work_sites: Option<WorkSites>,
    construction: Option<Construction>,
    goals: Option<Goals>,
    dreams: Option<Dreams>,
    nutrition: Option<Nutrition>,
    pending_food: Vec<FoodEffect>,
    travel: Option<Travel>,
    family: Option<Families>,
    care: Option<DependentCare>,
    agriculture: Option<Agriculture>,
    dynamics: Option<Dynamics>,
    missions: Option<Missions>,
    campaign: Option<Campaign>,
    skills: Option<Skills>,
    knowledge: Option<Knowledge>,
    combat: Option<game_parallel_economy_combat::Runtime>,
    fatigue: Option<Fatigue>,
    environment: Option<Environment>,
    purpose: Option<Purpose>,
    recovery: Option<RecoveryRules>,
    gathering: Option<Gathering>,
    food_search: Option<FoodSearch>,
    giving: Option<Giving>,
    medical: Option<Medical>,
    hauling: Option<Hauling>,
    epoch: u64,
}
pub struct Draft {
    epoch: u64,
    physical: AdvanceDraft,
    people: Vec<Person>,
    market: Option<Market>,
    agreements: Option<Agreements>,
    opinions: Option<Opinions>,
    autowork: Option<Autowork>,
    work_sites: Option<WorkSites>,
    construction: Option<Construction>,
    goals: Option<Goals>,
    dreams: Option<Dreams>,
    nutrition: Option<Nutrition>,
    pending_food: Vec<FoodEffect>,
    travel: Option<Travel>,
    family: Option<Families>,
    care: Option<DependentCare>,
    agriculture: Option<Agriculture>,
    dynamics: Option<Dynamics>,
    missions: Option<Missions>,
    campaign: Option<Campaign>,
    skills: Option<Skills>,
    knowledge: Option<Knowledge>,
    combat: Option<game_parallel_economy_combat::Draft>,
    fatigue: Option<game_parallel_economy_fatigue::Draft>,
    purpose: Option<Purpose>,
    gathering: Option<Gathering>,
    gathered: Option<bool>,
    food_search: Option<FoodSearch>,
    giving: Option<Giving>,
    hauling: Option<Hauling>,
    given: Option<bool>,
    treated_other: Option<bool>,
}
impl Draft {
    pub fn events(&self) -> &Events {
        self.physical.events()
    }
}
impl Economy {
    /// Change this person's willingness to build for their own production goals.
    pub fn set_goal_building_choice(&mut self, person: u64, enabled: bool) -> Result<(), String> {
        if self
            .people
            .binary_search_by_key(&person, |p| p.id)
            .ok()
            .is_none_or(|i| self.people[i].health == 0)
        {
            return Err("person must be alive".into());
        }
        let mut next = self
            .construction
            .clone()
            .ok_or("construction is disabled")?;
        next.goal_builders
            .iter_mut()
            .find(|p| p.person == person)
            .ok_or("person has no goal building preference")?
            .enabled = enabled;
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("economic generation overflow")?;
        self.construction = Some(next);
        self.epoch = epoch;
        Ok(())
    }
    /// Change this person's voluntary preference without changing felt pressure.
    pub fn set_demolition_choice(&mut self, person: u64, willing: bool) -> Result<(), String> {
        if self
            .people
            .binary_search_by_key(&person, |p| p.id)
            .ok()
            .is_none_or(|i| self.people[i].health == 0)
        {
            return Err("person must be alive".into());
        }
        let mut next = self
            .construction
            .clone()
            .ok_or("construction is disabled")?;
        next.demolition
            .iter_mut()
            .find(|p| p.person == person)
            .ok_or("person has no demolition preference")?
            .willing = willing;
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("economic generation overflow")?;
        self.construction = Some(next);
        self.epoch = epoch;
        Ok(())
    }
    /// Change only this person's voluntary construction preference.
    pub fn set_construction_choice(&mut self, person: u64, willing: bool) -> Result<(), String> {
        if self
            .people
            .binary_search_by_key(&person, |p| p.id)
            .ok()
            .is_none_or(|i| self.people[i].health == 0)
        {
            return Err("person must be alive".into());
        }
        let mut next = self
            .construction
            .clone()
            .ok_or("construction is disabled")?;
        next.policies
            .iter_mut()
            .find(|p| p.person == person)
            .ok_or("person has no construction preference")?
            .willing = willing;
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("economic generation overflow")?;
        self.construction = Some(next);
        self.epoch = epoch;
        Ok(())
    }
    /// Assemble an authored site blueprint using a real local or carried kit.
    pub fn build_site(&mut self, person: u64, blueprint: u32) -> Result<bool, String> {
        game_parallel_economy_campaign::preflight!(self);
        let draft = self.prepare_interaction(
            None,
            None,
            None,
            Some(game_parallel_economy_interaction::PhysicalChoice::Build { person, blueprint }),
        )?;
        self.commit(draft)?;
        Ok(self.construction.as_ref().is_some_and(|owner| {
            owner
                .events
                .iter()
                .any(|e| e.person == person && e.blueprint == blueprint && e.site.is_some())
        }))
    }
    /// Attempt to destroy an idle local site, without recovering its materials.
    pub fn destroy_site(&mut self, person: u64, site: u64) -> Result<bool, String> {
        game_parallel_economy_campaign::preflight!(self);
        let draft = self.prepare_interaction(
            None,
            None,
            None,
            Some(game_parallel_economy_interaction::PhysicalChoice::DestroySite { person, site }),
        )?;
        self.commit(draft)?;
        Ok(self
            .work_sites
            .as_ref()
            .is_some_and(|sites| sites.sites.binary_search_by_key(&site, |s| s.id).is_err()))
    }
    pub fn set_hauling_asking_choice(&mut self, person: u64, enabled: bool) -> Result<(), String> {
        if self
            .people
            .binary_search_by_key(&person, |p| p.id)
            .ok()
            .is_none_or(|i| self.people[i].health == 0)
        {
            return Err("person must be alive".into());
        }
        let mut next = self.hauling.clone().ok_or("hauling is disabled")?;
        next.asking
            .iter_mut()
            .find(|p| p.person == person)
            .ok_or("person has no hauling asking preference")?
            .enabled = enabled;
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("economic generation overflow")?;
        self.hauling = Some(next);
        self.epoch = epoch;
        Ok(())
    }
    /// Administrative access; player observations must filter to involved people.
    pub fn hauling_requests(&self) -> &[HaulingRequest] {
        self.hauling
            .as_ref()
            .map_or(&[], |hauling| &hauling.requests)
    }
    /// Ask for ongoing transport help; the carrier remains free to refuse or stop.
    pub fn ask_haul(
        &mut self,
        person: u64,
        carrier: u64,
        source: u64,
        destination: u64,
        good: u32,
        lifetime: u64,
    ) -> Result<(), String> {
        game_parallel_economy_campaign::preflight!(self);
        let draft = self.prepare_interaction(
            None,
            None,
            None,
            Some(game_parallel_economy_interaction::PhysicalChoice::AskHaul {
                person,
                carrier,
                source,
                destination,
                good,
                lifetime,
            }),
        )?;
        self.commit(draft)?;
        Ok(())
    }
    /// Change this person's inclination to haul; social pressure can still affect their choice.
    pub fn set_hauling_choice(&mut self, person: u64, willing: bool) -> Result<(), String> {
        if self
            .people
            .binary_search_by_key(&person, |p| p.id)
            .ok()
            .is_none_or(|i| self.people[i].health == 0)
        {
            return Err("person must be alive".into());
        }
        let mut next = self.hauling.clone().ok_or("hauling is disabled")?;
        next.policies
            .iter_mut()
            .find(|p| p.person == person)
            .ok_or("person has no hauling preference")?
            .willing = willing;
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("economic generation overflow")?;
        self.hauling = Some(next);
        self.epoch = epoch;
        Ok(())
    }
    /// Administrative access; a player view must filter to their own requests.
    pub fn food_requests(&self) -> &[FoodRequest] {
        self.giving.as_ref().map_or(&[], |giving| &giving.requests)
    }
    /// Change one individual's interpretations without changing anyone else's feelings.
    pub fn set_opinion_sensitivity(
        &mut self,
        sensitivity: OpinionSensitivity,
    ) -> Result<(), String> {
        if self
            .people
            .binary_search_by_key(&sensitivity.person, |p| p.id)
            .ok()
            .is_none_or(|i| self.people[i].health == 0)
        {
            return Err("person must be alive".into());
        }
        let mut next = self.opinions.clone().ok_or("opinions are disabled")?;
        if let Some(existing) = next
            .sensitivities
            .iter_mut()
            .find(|p| p.person == sensitivity.person)
        {
            *existing = sensitivity;
        } else {
            next.sensitivities.push(sensitivity);
        }
        let people: std::collections::HashSet<_> = self.people.par_iter().map(|p| p.id).collect();
        next.validate(&people, self.tick()).map_err(str::to_owned)?;
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("economic generation overflow")?;
        self.opinions = Some(next);
        self.epoch = epoch;
        Ok(())
    }
    /// Administrative relationship records; player views filter by observer.
    pub fn opinions(&self) -> &[Opinion] {
        self.opinions.as_ref().map_or(&[], |o| &o.records)
    }
    /// Administrative lookup; player observations expose only the current person's experience.
    pub fn sharing_learning(&self, person: u64) -> Option<&SharingLearning> {
        self.giving
            .as_ref()?
            .learning
            .iter()
            .find(|p| p.person == person)
    }
    /// Ask a nearby person for food; their response is their own later choice.
    pub fn ask_food(&mut self, person: u64, target: u64, lifetime: u64) -> Result<(), String> {
        game_parallel_economy_campaign::preflight!(self);
        let draft = self.prepare_interaction(
            None,
            None,
            None,
            Some(game_parallel_economy_interaction::PhysicalChoice::AskFood {
                person,
                target,
                lifetime,
            }),
        )?;
        self.commit(draft)?;
        Ok(())
    }
    /// Change an individual's inclination to ask, without binding the recipient.
    pub fn set_food_asking_choice(&mut self, person: u64, enabled: bool) -> Result<(), String> {
        if self
            .people
            .binary_search_by_key(&person, |p| p.id)
            .ok()
            .is_none_or(|i| self.people[i].health == 0)
        {
            return Err("person must be alive".into());
        }
        let mut next = self.giving.clone().ok_or("food requests are disabled")?;
        let policy = next
            .asking
            .iter_mut()
            .find(|p| p.person == person)
            .ok_or("person has no food asking preference")?;
        policy.enabled = enabled;
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("economic generation overflow")?;
        self.giving = Some(next);
        self.epoch = epoch;
        Ok(())
    }
    /// Change a person's own inclination to provide their configured care.
    pub fn set_medical_choice(&mut self, person: u64, willing: bool) -> Result<(), String> {
        if self
            .people
            .binary_search_by_key(&person, |p| p.id)
            .ok()
            .is_none_or(|i| self.people[i].health == 0)
        {
            return Err("person must be alive".into());
        }
        let mut next = self.medical.clone().ok_or("medical care is disabled")?;
        let policy = next
            .policies
            .iter_mut()
            .find(|p| p.person == person)
            .ok_or("person has no medical preference")?;
        policy.willing = willing;
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("economic generation overflow")?;
        self.medical = Some(next);
        self.epoch = epoch;
        Ok(())
    }
    /// Change a person's own inclination to give under their existing preference.
    pub fn set_giving_choice(&mut self, person: u64, willing: bool) -> Result<(), String> {
        if self
            .people
            .binary_search_by_key(&person, |p| p.id)
            .ok()
            .is_none_or(|i| self.people[i].health == 0)
        {
            return Err("person must be alive".into());
        }
        let mut next = self.giving.clone().ok_or("giving is disabled")?;
        if let Some(policy) = next.policies.iter_mut().find(|p| p.person == person) {
            policy.willing = willing;
        } else if let Some(policy) = next.responses.iter_mut().find(|p| p.person == person) {
            policy.willing = willing;
            if !willing {
                policy.affinity_threshold = None;
            }
        } else {
            return Err("person has no giving preference".into());
        }
        if !willing && let Some(learning) = next.learning.iter_mut().find(|p| p.person == person) {
            learning.enabled = false;
        }
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("economic generation overflow")?;
        self.giving = Some(next);
        self.epoch = epoch;
        Ok(())
    }
    /// Decide whether this person acts on, and continues to build, their own sharing experience.
    pub fn set_sharing_learning_choice(
        &mut self,
        person: u64,
        enabled: bool,
    ) -> Result<(), String> {
        if self
            .people
            .binary_search_by_key(&person, |p| p.id)
            .ok()
            .is_none_or(|i| self.people[i].health == 0)
        {
            return Err("person must be alive".into());
        }
        let mut next = self.giving.clone().ok_or("giving is disabled")?;
        let learning = next
            .learning
            .iter_mut()
            .find(|p| p.person == person)
            .ok_or("person has no sharing experience")?;
        learning.enabled = enabled;
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("economic generation overflow")?;
        self.giving = Some(next);
        self.epoch = epoch;
        Ok(())
    }
    /// Change one person's inclination to seek food in another area.
    pub fn set_food_search_choice(&mut self, person: u64, enabled: bool) -> Result<(), String> {
        if self
            .people
            .binary_search_by_key(&person, |p| p.id)
            .ok()
            .is_none_or(|i| self.people[i].health == 0)
        {
            return Err("person must be alive".into());
        }
        let mut next = self.food_search.clone().ok_or("food search is disabled")?;
        if let Some(preference) = next.people.iter_mut().find(|p| p.person == person) {
            preference.enabled = enabled;
            if !enabled {
                preference.target = None;
            }
        } else {
            next.people.push(FoodSearchPreference {
                person,
                enabled,
                next_review: 0,
                target: None,
            });
        }
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("economic generation overflow")?;
        self.food_search = Some(next);
        self.epoch = epoch;
        Ok(())
    }
    /// Administrative resource site access; player views must filter by location.
    pub fn resource_nodes(&self) -> &[ResourceNode] {
        self.gathering.as_ref().map_or(&[], |g| g.nodes.as_slice())
    }
    /// Change one person's own gathering preference and chosen site.
    pub fn set_gathering_choice(
        &mut self,
        person: u64,
        node: u64,
        willing: bool,
    ) -> Result<(), String> {
        if self
            .people
            .binary_search_by_key(&person, |p| p.id)
            .ok()
            .is_none_or(|i| self.people[i].health == 0)
        {
            return Err("person must be alive".into());
        }
        let mut next = self.gathering.clone().ok_or("gathering is disabled")?;
        if !next.nodes.iter().any(|site| site.id == node) {
            return Err("unknown resource site".into());
        }
        if let Some(policy) = next.policies.iter_mut().find(|p| p.person == person) {
            policy.node = node;
            policy.willing = willing;
        } else {
            next.policies.push(GatherPolicy {
                person,
                node,
                willing,
                pressured_by: None,
                pressure_threshold: None,
            });
        }
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("economic generation overflow")?;
        self.gathering = Some(next);
        self.epoch = epoch;
        Ok(())
    }
    /// Attempt to gather from a local resource site and advance one boundary.
    pub fn gather(&mut self, person: u64, node: u64) -> Result<bool, String> {
        game_parallel_economy_campaign::preflight!(self);
        let draft = self.prepare_interaction(
            None,
            None,
            None,
            Some(game_parallel_economy_interaction::PhysicalChoice::Gather { person, node }),
        )?;
        let admitted = draft.gathered.unwrap_or(false);
        self.commit(draft)?;
        Ok(admitted)
    }
    /// Give actual carried goods to a local person without requiring a promise.
    pub fn give(
        &mut self,
        person: u64,
        recipient: u64,
        good: u32,
        quantity: u64,
    ) -> Result<bool, String> {
        game_parallel_economy_campaign::preflight!(self);
        let draft = self.prepare_interaction(
            None,
            None,
            None,
            Some(game_parallel_economy_interaction::PhysicalChoice::Give {
                person,
                recipient,
                good,
                quantity,
            }),
        )?;
        let delivered = draft.given.unwrap_or(false);
        self.commit(draft)?;
        Ok(delivered)
    }
    /// Use carried medicine to try to treat a local person.
    pub fn treat_other(
        &mut self,
        person: u64,
        patient: u64,
        medicine: u32,
        quantity: u64,
    ) -> Result<bool, String> {
        game_parallel_economy_campaign::preflight!(self);
        let draft = self.prepare_interaction(
            None,
            None,
            None,
            Some(
                game_parallel_economy_interaction::PhysicalChoice::TreatOther {
                    person,
                    patient,
                    medicine,
                    quantity,
                },
            ),
        )?;
        let treated = draft.treated_other.unwrap_or(false);
        self.commit(draft)?;
        Ok(treated)
    }
    game_parallel_economy_read::read_method!();
    game_parallel_economy_promises::promise_methods!();
    game_parallel_economy_agriculture::cultivation_methods!();
    game_parallel_economy_person::work_choice_method!();
    game_parallel_economy_interaction::meal_method!();
    game_parallel_economy_interaction::transfer_method!();
    game_parallel_economy_interaction::fulfill_method!();
    game_parallel_economy_interaction::treat_method!();
    game_parallel_economy_claims_output::farm_method!();
    game_parallel_economy_fatigue::rest_method!();
    game_parallel_economy_interaction::combat_methods!();
    game_parallel_economy_goals::request_method!();
    game_parallel_economy_person::departure_method!();
    game_parallel_economy_dynamics::social_choice_method!();
    game_parallel_economy_claims::claim_methods!();
    game_parallel_economy_expeditions::mission_methods!();
    game_parallel_economy_campaign::campaign_methods!();
    game_parallel_economy_interaction::interaction_methods!();
    game_parallel_economy_interaction::work_methods!();
    game_parallel_economy_knowledge::knowledge_methods!();
    pub fn compile(mut state: State) -> Result<Self, String> {
        state.people.par_sort_unstable_by_key(|row| row.id);
        state
            .work_tools
            .par_sort_unstable_by_key(|row| (row.recipe, row.good));
        if state.rules.carrying_capacity == 0
            || state.rules.max_health == 0
            || state.rules.meal_threshold == 0
            || state
                .people
                .par_windows(2)
                .any(|rows| rows[0].id == rows[1].id)
            || state.people.par_iter().any(|person| {
                person.id == 0 || person.region == Some(0) || person.health > state.rules.max_health
            })
        {
            return Err("invalid personal economic state".into());
        }
        let physical = Runtime::compile(state.physical)?;
        let people: Vec<_> = state
            .people
            .par_iter()
            .map(|row| (row.id, row.region, row.health, row.hunger))
            .collect();
        let ids: HashSet<_> = people.iter().map(|row| row.0).collect();
        game_parallel_economy_fatigue::validate_world!(state, ids);
        if state
            .recovery
            .as_ref()
            .is_some_and(|rules| rules.interval == 0)
        {
            return Err("invalid recovery rules".into());
        }
        if let Some(purpose) = &state.purpose {
            purpose.validate(&ids).map_err(str::to_owned)?;
        }
        let goods: HashSet<_> = physical.goods().iter().map(|row| row.id).collect();
        game_parallel_economy_skills::validate_world!(state, physical, ids);
        let view = physical.view(physical.tick())?;
        environment::validate(state.environment.as_ref(), &view, &goods)?;
        if let Some(gathering) = &state.gathering {
            gathering.validate(&ids, &goods, &view)?;
        }
        if let Some(search) = &state.food_search {
            search.validate(&ids, state.knowledge.as_ref(), state.travel.as_ref())?;
        }
        if let Some(giving) = &state.giving {
            giving.validate(&ids, physical.goods())?;
        }
        if let Some(hauling) = &state.hauling {
            hauling.validate(&ids, &goods, &view, state.travel.as_ref())?;
        }
        if let Some(medical) = &state.medical {
            medical.validate(
                &ids,
                &goods,
                state.dynamics.as_ref().and_then(|d| d.disease.as_ref()),
            )?;
        }
        let combat = game_parallel_economy_combat::compile_world!(state, physical, view);
        game_parallel_economy_knowledge::validate_world!(state, ids, view, physical);
        game_parallel_economy_dynamics::validate_world!(state, ids, goods, view, physical);
        game_parallel_economy_expeditions::validate_world!(state, physical, view, ids);
        let _ = game_parallel_lot_projection::cargo(&view, state.rules.carrying_capacity)?;
        family::validate_custody!(people, view, ids, physical);
        if let Some(travel) = &state.travel {
            person::travel::validate_world(travel, &state.people, &view, physical.work())?;
        }
        family::validate_care!(state, physical, ids);
        game_parallel_economy_campaign::validate_world!(state, physical, view);
        game_parallel_economy_agriculture::validate_world!(state, physical, view);
        if let Some(market) = &state.market {
            market.validate_index(&people, &goods, physical.tick())?;
        }
        if let Some(agreements) = &state.agreements {
            agreements
                .validate(&ids, &goods, physical.tick())
                .map_err(str::to_owned)?;
            if agreements
                .offering
                .par_iter()
                .any(|p| p.after_work_recipe.is_some())
            {
                let recipes: HashSet<_> = physical.recipes().par_iter().map(|r| r.id).collect();
                if agreements
                    .offering
                    .par_iter()
                    .any(|p| p.after_work_recipe.is_some_and(|id| !recipes.contains(&id)))
                {
                    return Err("unknown remembered-work recipe".into());
                }
            }
        }
        if let Some(opinions) = &state.opinions {
            opinions
                .validate(&ids, physical.tick())
                .map_err(str::to_owned)?;
        }
        if let Some(nutrition) = &state.nutrition {
            nutrition.validate(&ids).map_err(str::to_owned)?;
        }
        game_parallel_lot_diet::validate_pending(&state.pending_food, &ids, physical.tick())?;
        game_parallel_economy_goals::validate(
            state.goals.as_ref(),
            state.dreams.as_ref(),
            state.autowork.is_some(),
            &ids,
            physical.recipes(),
            physical.tick(),
        )?;
        if let Some(sites) = &mut state.work_sites {
            sites.normalize()?;
            let recipes: HashSet<_> = physical.recipes().par_iter().map(|r| r.id).collect();
            let regions: HashSet<_> = view
                .containers
                .par_iter()
                .filter_map(|c| match c.place {
                    game_parallel_lots::Place::Stock { region, .. } => Some(region),
                    _ => None,
                })
                .collect();
            sites.validate(physical.work(), &recipes, &regions)?;
        }
        if let Some(claims) = state.dynamics.as_ref().and_then(|d| d.claims.as_ref())
            && (claims.site_claims.par_iter().any(|claim| {
                state
                    .work_sites
                    .as_ref()
                    .is_none_or(|sites| claim.site >= sites.next_id)
            }) || claims.site_events.par_iter().any(|event| {
                state
                    .work_sites
                    .as_ref()
                    .is_none_or(|sites| event.site >= sites.next_id)
            }))
        {
            return Err("invalid personal site claim".into());
        }
        if let Some(owner) = &mut state.construction {
            owner.normalize();
            let sites = state
                .work_sites
                .as_ref()
                .ok_or("construction requires work sites")?;
            owner.validate(&ids, &goods, sites, physical.tick())?;
        }
        if !state.work_tools.is_empty() {
            let recipes: HashSet<_> = physical.recipes().par_iter().map(|r| r.id).collect();
            game_parallel_economy_work::tools::validate(&state.work_tools, &recipes, &goods)?;
        }
        if let Some(auto) = &state.autowork {
            let production = game_parallel_lot_projection::production(
                &physical,
                &view,
                state.rules.max_work_hunger,
            );
            auto.validate(&people, &production)?;
            if auto.requests.as_ref().is_some_and(|requests| {
                requests
                    .memories
                    .par_iter()
                    .any(|m| m.last_success_at.is_some_and(|t| t > physical.tick()))
            }) {
                return Err("work memory is from the future".into());
            }
            game_parallel_economy_work::validate_links(auto, physical.work())?;
        }
        Ok(Self {
            physical,
            seed: state.seed,
            people: state.people,
            rules: state.rules,
            market: state.market,
            agreements: state.agreements,
            opinions: state.opinions,
            autowork: state.autowork,
            work_tools: state.work_tools,
            work_sites: state.work_sites,
            construction: state.construction,
            goals: state.goals,
            dreams: state.dreams,
            nutrition: state.nutrition,
            pending_food: state.pending_food,
            travel: state.travel,
            family: state.family,
            care: state.care,
            agriculture: state.agriculture,
            dynamics: state.dynamics,
            missions: state.missions,
            campaign: state.campaign,
            skills: state.skills,
            knowledge: state.knowledge,
            combat,
            fatigue: state.fatigue,
            environment: state.environment,
            purpose: state.purpose,
            recovery: state.recovery,
            gathering: state.gathering,
            food_search: state.food_search,
            giving: state.giving,
            medical: state.medical,
            hauling: state.hauling,
            epoch: 0,
        })
    }
    pub fn snapshot(&self) -> State {
        State {
            seed: self.seed,
            physical: self.physical.snapshot(),
            people: self.people.clone(),
            rules: self.rules.clone(),
            market: self.market.clone(),
            agreements: self.agreements.clone(),
            opinions: self.opinions.clone(),
            autowork: self.autowork.clone(),
            work_tools: self.work_tools.clone(),
            work_sites: self.work_sites.clone(),
            construction: self.construction.clone(),
            goals: self.goals.clone(),
            dreams: self.dreams.clone(),
            nutrition: self.nutrition.clone(),
            pending_food: self.pending_food.clone(),
            travel: self.travel.clone(),
            family: self.family.clone(),
            care: self.care.clone(),
            agriculture: self.agriculture.clone(),
            dynamics: self.dynamics.clone(),
            missions: self.missions.clone(),
            campaign: self.campaign.clone(),
            skills: self.skills.clone(),
            knowledge: self.knowledge.clone(),
            combat: self.combat.as_ref().map(|combat| combat.state.clone()),
            fatigue: self.fatigue.clone(),
            environment: self.environment.clone(),
            purpose: self.purpose.clone(),
            recovery: self.recovery.clone(),
            gathering: self.gathering.clone(),
            food_search: self.food_search.clone(),
            giving: self.giving.clone(),
            medical: self.medical.clone(),
            hauling: self.hauling.clone(),
        }
    }
    pub fn tick(&self) -> u64 {
        self.physical.tick()
    }
    pub fn prepare(&self) -> Result<Draft, String> {
        self.prepare_interaction(None, None, None, None)
    }
    fn prepare_interaction(
        &self,
        request: Option<game_parallel_economy_interaction::Request>,
        player_work: Option<game_parallel_economy_interaction::WorkAction>,
        player_meal: Option<person::meal::Choice>,
        player_transfer: Option<game_parallel_economy_interaction::PhysicalChoice>,
    ) -> Result<Draft, String> {
        let tick = self.tick().checked_add(1).ok_or("tick overflow")?;
        let campaign_draft = game_parallel_economy_campaign::prepare_world!(self);
        let view = self.physical.view(tick)?;
        let destroy_site =
            if let Some(game_parallel_economy_interaction::PhysicalChoice::DestroySite {
                person,
                site,
            }) = player_transfer
            {
                let actor = self
                    .people
                    .binary_search_by_key(&person, |p| p.id)
                    .ok()
                    .map(|i| &self.people[i])
                    .ok_or("unknown person")?;
                let sites = self.work_sites.as_ref().ok_or("work sites are disabled")?;
                let target = sites
                    .sites
                    .binary_search_by_key(&site, |s| s.id)
                    .ok()
                    .map(|i| &sites.sites[i])
                    .ok_or("unknown work site")?;
                if !actor.controlled || actor.health == 0 || actor.region != Some(target.region) {
                    return Err(
                        "site destruction requires a living controlled person at the site".into(),
                    );
                }
                if self.family.as_ref().is_some_and(|f| {
                    f.members
                        .iter()
                        .any(|m| m.id == person && m.age < f.rules.min_work_age)
                }) {
                    return Err("person is too young to destroy a site".into());
                }
                Some((
                    site,
                    sites
                        .bindings
                        .par_iter()
                        .any(|binding| binding.site == site),
                ))
            } else {
                None
            };
        let claims = game_parallel_economy_claims::prepare_index!(self);
        let site_classes = claims
            .as_ref()
            .map_or_else(Vec::new, |index| index.site_classes());
        let (fighters, resting, people, meal_candidates, meals, mut busy) = family::prepare_people!(
            self,
            view,
            request,
            player_work,
            player_meal,
            player_transfer
        );
        let interaction =
            game_parallel_economy_interaction::prepare_world!(self, request, busy, tick);
        let mut action_base = u64::try_from(meals.actions().len())
            .map_err(|_| "action count overflow")?
            .checked_add(1)
            .ok_or("action count overflow")?;
        let care = family::prepare_care!(
            self,
            people,
            view,
            claims,
            busy,
            meal_candidates,
            action_base,
            tick
        );
        let (pickup, mut claim_uses, transfer) =
            family::prepare_provisioning!(self, view, busy, action_base, claims, player_transfer);
        let exchange = game_parallel_economy_exchange::Context {
            market: self.market.as_ref(),
            view: &view,
            capacity: self.rules.carrying_capacity,
            people: &people,
            goods: self.physical.goods(),
            opinions: self.opinions.as_ref(),
        };
        let trade = exchange.trade(&mut busy, action_base)?;
        action_base = action_base
            .checked_add(game_parallel_economy_exchange::actions(trade.as_ref()).len() as u64)
            .ok_or("action count overflow")?;
        let needs_locations = self.agreements.as_ref().is_some_and(|agreements| {
            !agreements.contracts.is_empty() || !agreements.offering.is_empty()
        }) || self.giving.is_some()
            || self.medical.is_some()
            || self.hauling.as_ref().is_some_and(|h| !h.asking.is_empty())
            || matches!(
                player_transfer,
                Some(
                    game_parallel_economy_interaction::PhysicalChoice::Give { .. }
                        | game_parallel_economy_interaction::PhysicalChoice::TreatOther { .. }
                        | game_parallel_economy_interaction::PhysicalChoice::AskFood { .. }
                        | game_parallel_economy_interaction::PhysicalChoice::AskHaul { .. }
                )
            );
        let locations = person::travel::Colocation::new(
            needs_locations.then_some(self.travel.as_ref()).flatten(),
        );
        let promises = game_parallel_economy_promises::plan_world!(
            self,
            people,
            locations,
            busy,
            tick,
            action_base,
            player_transfer
        );
        let proposals = exchange.proposals(&mut busy)?;
        action_base = action_base
            .checked_add(promises.action_count() as u64)
            .ok_or("action count overflow")?;
        let treatment = game_parallel_economy_dynamics::prepare_treatment!(
            self,
            busy,
            view,
            action_base,
            player_transfer
        );
        action_base = action_base
            .checked_add(treatment.actions.len() as u64)
            .ok_or("action count overflow")?;
        game_parallel_economy_work::reserve(&mut busy, treatment.actors.iter().copied());
        let manual_give = match player_transfer {
            Some(game_parallel_economy_interaction::PhysicalChoice::Give {
                person,
                recipient,
                good,
                quantity,
            }) => Some((person, recipient, good, quantity)),
            _ => None,
        };
        game_parallel_economy_work::reserve_underage(self.family.as_ref(), &mut busy);
        if let Some((person, _, _, _)) = manual_give
            && self.family.as_ref().is_some_and(|f| {
                f.members
                    .iter()
                    .any(|m| m.id == person && m.age < f.rules.min_work_age)
            })
        {
            return Err("person is too young to give possessions".into());
        }
        let promise_offers = game_parallel_economy_promises::offers::prepare(
            game_parallel_economy_promises::offers::Context {
                owner: self.agreements.as_ref(),
                people: &self.people,
                opinions: self.opinions.as_ref(),
                requests: self
                    .autowork
                    .as_ref()
                    .and_then(|auto| auto.requests.as_ref()),
                locations: &locations,
                busy: &busy,
                max_hunger: self.rules.max_work_hunger,
                tick,
            },
        )?;
        if !promise_offers.proposals().is_empty() {
            busy.par_extend(promise_offers.proposals().par_iter().map(|p| p.debtor));
            if busy.par_windows(2).any(|rows| rows[0] > rows[1]) {
                busy.par_sort_unstable();
            }
            if busy.par_windows(2).any(|rows| rows[0] == rows[1]) {
                busy = busy
                    .par_chunk_by(|a, b| a == b)
                    .map(|group| group[0])
                    .collect();
            }
        }
        let mut next_giving = self.giving.clone();
        if let Some(giving) = &mut next_giving {
            let askers = giving.ask(
                &self.people,
                self.physical.goods(),
                &view,
                &busy,
                tick,
                &locations,
            )?;
            game_parallel_economy_work::reserve(&mut busy, askers);
            let acquaintances = self
                .dynamics
                .as_ref()
                .and_then(|owner| owner.social.as_ref())
                .map_or(&[][..], |social| social.relationships.as_slice());
            let askers = giving_discovery::prepare(
                giving,
                giving_discovery::Context {
                    people: &self.people,
                    goods: self.physical.goods(),
                    view: &view,
                    busy: &busy,
                    relationships: acquaintances,
                    opinions: self.opinions.as_ref(),
                    locations: &locations,
                    seed: self.seed,
                    tick,
                },
            )?;
            game_parallel_economy_work::reserve(&mut busy, askers);
        }
        if let Some(game_parallel_economy_interaction::PhysicalChoice::AskFood {
            person,
            target,
            lifetime,
        }) = player_transfer
        {
            let giving = next_giving.as_mut().ok_or("food requests are disabled")?;
            let actor = self
                .people
                .binary_search_by_key(&person, |p| p.id)
                .ok()
                .map(|i| &self.people[i])
                .ok_or("unknown person")?;
            let recipient = self
                .people
                .binary_search_by_key(&target, |p| p.id)
                .ok()
                .map(|i| &self.people[i])
                .ok_or("unknown target")?;
            if !actor.controlled
                || actor.health == 0
                || recipient.health == 0
                || person == target
                || locations
                    .region((actor.id, actor.region), (recipient.id, recipient.region))
                    .is_none()
                || lifetime == 0
            {
                return Err(
                    "food request requires a living controlled person and a local recipient".into(),
                );
            }
            let expires_at = tick
                .checked_add(lifetime)
                .ok_or("food request expiry overflow")?;
            giving.requests = giving
                .requests
                .par_iter()
                .filter(|r| r.requester != person)
                .cloned()
                .collect();
            giving.requests.push(FoodRequest {
                requester: person,
                target,
                expires_at,
            });
        }
        let giving_plan = giving::Context {
            owner: self.giving.as_ref(),
            people: &self.people,
            opinions: self.opinions.as_ref(),
            goods: self.physical.goods(),
            view: &view,
            busy: &busy,
            manual: manual_give,
            tick,
            seed: self.seed,
            locations: &locations,
        }
        .prepare(action_base)?;
        action_base = action_base
            .checked_add(giving_plan.actions.len() as u64)
            .ok_or("action count overflow")?;
        if !giving_plan.actors.is_empty() {
            busy.par_extend(giving_plan.actors.par_iter().copied());
            busy.par_sort_unstable();
            busy = busy
                .par_chunk_by(|a, b| a == b)
                .map(|group| group[0])
                .collect();
        }
        let manual_medical = match player_transfer {
            Some(game_parallel_economy_interaction::PhysicalChoice::TreatOther {
                person,
                patient,
                medicine,
                quantity,
            }) => Some((person, patient, medicine, quantity)),
            _ => None,
        };
        if let Some((person, _, _, _)) = manual_medical
            && self.family.as_ref().is_some_and(|f| {
                f.members
                    .iter()
                    .any(|m| m.id == person && m.age < f.rules.min_work_age)
            })
        {
            return Err("person is too young to provide medical care".into());
        }
        let mut medical_blocked: Vec<_> = treatment
            .actors
            .par_iter()
            .copied()
            .chain(fighters.par_iter().copied())
            .chain(meal_candidates.par_iter().map(|row| row.0))
            .collect();
        medical_blocked.extend(request.map(|choice| choice.actor));
        medical_blocked.extend(player_work.map(|choice| choice.actor()));
        medical_blocked.extend(player_transfer.and_then(|choice| {
            (!matches!(
                choice,
                game_parallel_economy_interaction::PhysicalChoice::Rest { .. }
            ))
            .then_some(choice.person())
        }));
        medical_blocked.par_sort_unstable();
        medical_blocked.dedup();
        let medical_plan = medical::Context {
            owner: self.medical.as_ref(),
            disease: self.dynamics.as_ref().and_then(|d| d.disease.as_ref()),
            people: &self.people,
            opinions: self.opinions.as_ref(),
            goods: self.physical.goods(),
            view: &view,
            busy: &busy,
            blocked: &medical_blocked,
            resting: &resting,
            seed: self.seed,
            tick,
            manual: manual_medical,
            locations: &locations,
        }
        .prepare(action_base)?;
        action_base = action_base
            .checked_add(medical_plan.actions.len() as u64)
            .ok_or("action count overflow")?;
        if !medical_plan.actors.is_empty() {
            busy.par_extend(medical_plan.actors.par_iter().copied());
            busy.par_sort_unstable();
            busy = busy
                .par_chunk_by(|a, b| a == b)
                .map(|group| group[0])
                .collect();
        }
        let mut next_hauling = self.hauling.clone();
        if let Some(hauling) = &mut next_hauling {
            hauling.requests = hauling
                .requests
                .par_iter()
                .filter(|r| r.expires_at >= tick)
                .cloned()
                .collect();
            let askers = hauling.ask(&self.people, &busy, &locations, tick)?;
            if !askers.is_empty() {
                busy.par_extend(askers.into_par_iter());
                busy.par_sort_unstable();
                busy = busy
                    .par_chunk_by(|a, b| a == b)
                    .map(|group| group[0])
                    .collect();
            }
        }
        if let Some(game_parallel_economy_interaction::PhysicalChoice::AskHaul {
            person,
            carrier,
            source,
            destination,
            good,
            lifetime,
        }) = player_transfer
        {
            let hauling = next_hauling.as_mut().ok_or("hauling is disabled")?;
            let actor = self
                .people
                .binary_search_by_key(&person, |p| p.id)
                .ok()
                .map(|i| &self.people[i])
                .ok_or("unknown person")?;
            let recipient = self
                .people
                .binary_search_by_key(&carrier, |p| p.id)
                .ok()
                .map(|i| &self.people[i])
                .ok_or("unknown carrier")?;
            let travel = self.travel.as_ref().ok_or("travel is disabled")?;
            let routes =
                game_parallel_economy_person::travel::routes(travel, &[source, destination]);
            if !actor.controlled
                || actor.health == 0
                || recipient.health == 0
                || person == carrier
                || lifetime == 0
                || locations
                    .region((actor.id, actor.region), (recipient.id, recipient.region))
                    .is_none()
                || source == destination
                || !routes.contains_key(&(source, destination))
                || !routes.contains_key(&(destination, source))
                || view
                    .container(game_parallel_lots::Place::Stock {
                        region: source,
                        good,
                    })
                    .is_none()
                || view
                    .container(game_parallel_lots::Place::Stock {
                        region: destination,
                        good,
                    })
                    .is_none()
            {
                return Err(
                    "hauling request requires a nearby person and valid route and stocks".into(),
                );
            }
            let expires_at = tick
                .checked_add(lifetime)
                .ok_or("hauling request expiry overflow")?;
            hauling.requests = hauling
                .requests
                .par_iter()
                .filter(|r| r.requester != person)
                .cloned()
                .collect();
            hauling.requests.push(HaulingRequest {
                requester: person,
                carrier,
                source,
                destination,
                good,
                expires_at,
            });
        }
        let hauling_plan = self
            .hauling
            .as_ref()
            .map(|owner| {
                hauling::Context {
                    owner,
                    travel: self.travel.as_ref().ok_or("hauling needs travel routes")?,
                    people: &self.people,
                    opinions: self.opinions.as_ref(),
                    claims: claims.as_ref(),
                    view: &view,
                    busy: &busy,
                    max_hunger: self.rules.max_work_hunger,
                    tick,
                    seed: self.seed,
                }
                .prepare(action_base)
            })
            .transpose()?;
        if let Some(plan) = &hauling_plan {
            action_base = action_base
                .checked_add(plan.actions.len() as u64)
                .ok_or("action count overflow")?;
            claim_uses.par_extend(plan.uses.par_iter().copied());
            if !plan.actors.is_empty() {
                busy.par_extend(plan.actors.par_iter().copied());
                busy.par_sort_unstable();
                busy = busy
                    .par_chunk_by(|a, b| a == b)
                    .map(|group| group[0])
                    .collect();
            }
        }
        let expedition =
            game_parallel_economy_expeditions::prepare_world!(self, people, busy, tick);
        let search_plan = self
            .food_search
            .as_ref()
            .zip(self.knowledge.as_ref())
            .zip(self.travel.as_ref())
            .map(|((owner, knowledge), travel)| {
                food_search::Context {
                    owner,
                    knowledge,
                    travel,
                    people: &self.people,
                    goods: self.physical.goods(),
                    view: &view,
                    claims: claims.as_ref(),
                    busy: &busy,
                    now: self.tick(),
                }
                .prepare()
            });
        if let Some(plan) = &search_plan
            && !plan.departures.is_empty()
        {
            busy.par_extend(plan.departures.par_iter().map(|d| d.person));
            busy.par_sort_unstable();
            busy = busy
                .par_chunk_by(|a, b| a == b)
                .map(|group| group[0])
                .collect();
        }
        let (mut farming, mut claim_outputs) = game_parallel_economy_claims_output::prepare_farming!(
            self,
            busy,
            view,
            tick,
            action_base,
            claims,
            player_transfer
        );
        action_base = action_base
            .checked_add(farming.as_ref().map_or(0, |p| p.changes.len() as u64))
            .ok_or("action count overflow")?;
        let manual_gather = match player_transfer {
            Some(game_parallel_economy_interaction::PhysicalChoice::Gather { person, node }) => {
                Some((person, node))
            }
            _ => None,
        };
        if let Some((person, _)) = manual_gather
            && (self.gathering.is_none()
                || self.family.as_ref().is_some_and(|f| {
                    f.members
                        .iter()
                        .any(|m| m.id == person && m.age < f.rules.min_work_age)
                })
                || self.physical.work().iter().any(|w| {
                    w.job.worker == person && w.phase == game_parallel_lot_runtime::Phase::Working
                }))
        {
            return Err("person cannot gather now".into());
        }
        let gathering_plan = self
            .gathering
            .as_ref()
            .map(|owner| {
                gathering::Context {
                    owner,
                    people: &self.people,
                    opinions: self.opinions.as_ref(),
                    busy: &busy,
                    goods: self.physical.goods(),
                    view: &view,
                    tick_millis: self.physical.tick_millis(),
                    seed: self.seed,
                    tick,
                    manual: manual_gather,
                }
                .prepare(action_base)
            })
            .transpose()?;
        if let Some(plan) = &gathering_plan {
            if !plan.actors.is_empty() {
                busy.par_extend(plan.actors.par_iter().copied());
                busy.par_sort_unstable();
                busy = busy
                    .par_chunk_by(|a, b| a == b)
                    .map(|group| group[0])
                    .collect();
            }
            if let Some(claims) = self
                .dynamics
                .as_ref()
                .and_then(|d| d.claims.as_ref())
                .filter(|c| c.preferences.par_iter().any(|p| p.assert_outputs))
            {
                let outputs = plan.outputs(self.gathering.as_ref().expect("gathering owner"));
                claim_outputs.par_extend(game_parallel_economy_claims_output::gather_outputs(
                    &outputs,
                    &self.people,
                    Some(claims),
                ));
            }
        }
        let (mut rates, abandon) =
            family::work_rates!(self, meal_candidates, player_work, fighters, resting);
        let mut goal_work = None;
        let goal_building = self
            .construction
            .as_ref()
            .is_some_and(|c| c.goal_builders.par_iter().any(|p| p.enabled));
        if !self.work_tools.is_empty() || goal_building {
            goal_work = self
                .goals
                .as_ref()
                .zip(self.autowork.as_ref())
                .map(|(goals, auto)| goals.policies(auto, tick.saturating_sub(1)));
            if self.goals.as_ref().is_some_and(|g| g.plan_inputs)
                && let (Some(planned), Some(base)) = (&mut goal_work, &self.autowork)
            {
                let production = game_parallel_lot_projection::production(
                    &self.physical,
                    &view,
                    self.rules.max_work_hunger,
                );
                let review = tick.saturating_sub(1)
                    / self.goals.as_ref().expect("goal planning").retry_interval;
                game_parallel_goals::planning::plan_at(planned, base, &production, &people, review);
            }
        }
        let mut goal_needs: Vec<_> = if goal_building {
            let explicit: std::collections::HashSet<_> = self
                .autowork
                .as_ref()
                .map_or_else(std::collections::HashSet::new, |auto| {
                    auto.policies.par_iter().map(|p| p.person).collect()
                });
            goal_work.as_ref().map_or_else(Vec::new, |planned| {
                planned
                    .policies
                    .par_iter()
                    .filter(|p| p.enabled && !explicit.contains(&p.person))
                    .map(|p| game_parallel_economy_building::GoalNeed {
                        person: p.person,
                        recipe: p.recipe,
                    })
                    .collect()
            })
        } else {
            Vec::new()
        };
        goal_needs.par_sort_unstable_by_key(|need| need.person);
        let mut goal_building_refusals = if goal_building {
            self.dynamics
                .as_ref()
                .and_then(|d| d.personality.as_ref())
                .map_or_else(Vec::new, |p| p.work_refusals(self.seed, tick))
        } else {
            Vec::new()
        };
        goal_building_refusals.par_sort_unstable();
        if goal_building
            && self.goals.as_ref().is_some_and(|g| g.plan_inputs)
            && !goal_needs.is_empty()
            && let (Some(owner), Some(sites), Some(planned)) = (
                self.construction.as_ref(),
                self.work_sites.as_ref(),
                &mut goal_work,
            )
        {
            let context = game_parallel_economy_building::Context {
                owner,
                sites,
                people: &self.people,
                view: &view,
                opinions: self.opinions.as_ref(),
                claims: claims.as_ref(),
                busy: &busy,
                max_hunger: self.rules.max_work_hunger,
                tick,
                manual: None,
                goal_needs: &goal_needs,
                unwilling: &goal_building_refusals,
            };
            let mut pending: Vec<_> = self
                .physical
                .work()
                .par_iter()
                .filter(|row| row.phase != game_parallel_lot_runtime::Phase::Working)
                .map(|row| row.job.worker)
                .collect();
            pending.par_sort_unstable();
            let production = game_parallel_lot_projection::production(
                &self.physical,
                &view,
                self.rules.max_work_hunger,
            );
            let review =
                tick.saturating_sub(1) / self.goals.as_ref().expect("goal planning").retry_interval;
            let steps = construction_planning::prepare(context, &production, &pending, review);
            planned.policies.par_iter_mut().for_each(|policy| {
                if let Ok(index) = steps.work.binary_search_by_key(&policy.person, |row| row.0) {
                    policy.recipe = steps.work[index].1;
                }
            });
            goal_needs = steps.builds;
        }
        if !self.work_tools.is_empty()
            || self.construction.is_some()
            || matches!(
                player_transfer,
                Some(game_parallel_economy_interaction::PhysicalChoice::Build { .. })
            )
        {
            action_base = action_base
                .checked_add(
                    gathering_plan
                        .as_ref()
                        .map_or(0, |plan| plan.actions.len() as u64),
                )
                .ok_or("action count overflow")?;
        }
        let tool_pickup = if !self.work_tools.is_empty() {
            {
                let mut unwilling = self
                    .dynamics
                    .as_ref()
                    .and_then(|d| d.personality.as_ref())
                    .map_or_else(Vec::new, |p| p.work_refusals(self.seed, tick));
                unwilling.par_sort_unstable();
                let policies = goal_work
                    .as_ref()
                    .or(self.autowork.as_ref())
                    .map_or(&[][..], |a| a.policies.as_slice());
                let requests = self
                    .autowork
                    .as_ref()
                    .and_then(|a| a.requests.as_ref())
                    .filter(|r| !r.pending.is_empty())
                    .map(|r| r.prepare(&people, self.opinions.as_ref(), &busy, tick));
                let plan = game_parallel_economy_work::tools::AcquisitionContext {
                    requirements: &self.work_tools,
                    auto: self.autowork.as_ref(),
                    policies,
                    requests: requests.as_ref(),
                    work: self.physical.work(),
                    rates: &rates,
                    people: &self.people,
                    view: &view,
                    claims: claims.as_ref(),
                    busy: &busy,
                    unwilling: &unwilling,
                    max_hunger: self.rules.max_work_hunger,
                }
                .prepare(action_base)?;
                game_parallel_economy_work::reserve(&mut busy, plan.actors.iter().copied());
                Some(plan)
            }
        } else {
            None
        };
        if let Some(plan) = &tool_pickup {
            action_base = action_base
                .checked_add(plan.actions.len() as u64)
                .ok_or("action count overflow")?;
        }
        let manual_build = match player_transfer {
            Some(game_parallel_economy_interaction::PhysicalChoice::Build {
                person,
                blueprint,
            }) => Some(game_parallel_economy_building::Manual { person, blueprint }),
            _ => None,
        };
        if manual_build.is_some_and(|m| {
            self.family.as_ref().is_some_and(|f| {
                f.members
                    .iter()
                    .any(|member| member.id == m.person && member.age < f.rules.min_work_age)
            })
        }) {
            return Err("person is too young to build".into());
        }
        if manual_build.is_some() && self.construction.is_none() {
            return Err("construction is disabled".into());
        }
        let mut construction_plan = self
            .construction
            .as_ref()
            .map(|owner| {
                let sites = self
                    .work_sites
                    .as_ref()
                    .expect("validated construction sites");
                game_parallel_economy_building::Context {
                    owner,
                    sites,
                    people: &self.people,
                    view: &view,
                    opinions: self.opinions.as_ref(),
                    claims: claims.as_ref(),
                    busy: &busy,
                    max_hunger: self.rules.max_work_hunger,
                    tick,
                    manual: manual_build,
                    goal_needs: &goal_needs,
                    unwilling: &goal_building_refusals,
                }
                .prepare(action_base)
            })
            .transpose()?;
        if let Some(plan) = &construction_plan {
            game_parallel_economy_work::reserve(&mut busy, plan.actors.iter().copied());
        }
        let demolition_plan = self
            .construction
            .as_ref()
            .zip(self.work_sites.as_ref())
            .map(|(owner, sites)| {
                game_parallel_economy_building::demolition::Context {
                    policies: &owner.demolition,
                    sites,
                    people: &self.people,
                    opinions: self.opinions.as_ref(),
                    claims: claims.as_ref(),
                    busy: &busy,
                    manual_site: destroy_site.map(|(site, _)| site),
                    max_hunger: self.rules.max_work_hunger,
                    seed: self.seed,
                    tick,
                }
                .prepare()
            });
        if let Some(plan) = &demolition_plan
            && !plan.actors.is_empty()
        {
            game_parallel_economy_work::reserve(&mut busy, plan.actors.iter().copied());
        }
        game_parallel_economy_claims::include_care_farming!(claim_uses, care, farming);
        let mut actions: Vec<Transaction> = meals.actions().to_vec();
        if let Some(plan) = tool_pickup {
            actions.extend(plan.actions);
            claim_uses.extend(plan.uses);
        }
        if let Some(plan) = &mut construction_plan {
            actions.append(&mut plan.actions);
            claim_uses.append(&mut plan.uses);
        }
        if let Some(plan) = &care {
            actions.extend_from_slice(&plan.actions);
        }
        actions.extend(pickup.actions);
        actions.extend(transfer);
        actions.extend_from_slice(game_parallel_economy_exchange::actions(trade.as_ref()));
        if let Some(plan) = &promises.delivery {
            actions.extend_from_slice(plan.actions());
        }
        actions.extend_from_slice(&treatment.actions);
        actions.extend_from_slice(&giving_plan.actions);
        actions.extend_from_slice(&medical_plan.actions);
        if let Some(plan) = &hauling_plan {
            actions.extend_from_slice(&plan.actions);
        }
        let compact =
            game_parallel_economy_agriculture::append_planting(&mut farming, &mut actions);
        let work = game_parallel_economy_goals::prepare_work!(
            self,
            view,
            people,
            claims,
            busy,
            tick,
            goal_work,
            site_classes
        );
        let starts =
            game_parallel_economy_claims::work_actions!(work, claims, self, claim_uses, busy);
        let (starts, manual_job) = game_parallel_economy_interaction::work_starts!(
            self,
            player_work,
            work,
            starts,
            claims,
            claim_uses
        );
        let starts = if self.work_tools.is_empty() {
            starts
        } else {
            rates = rates
                .into_par_iter()
                .filter(|(id, _)| {
                    self.physical
                        .work()
                        .binary_search_by_key(id, |w| w.job.id)
                        .ok()
                        .is_some_and(|i| {
                            let job = &self.physical.work()[i].job;
                            game_parallel_economy_work::tools::available(
                                &self.work_tools,
                                &view,
                                job.worker,
                                job.recipe,
                            )
                        })
                })
                .collect();
            std::borrow::Cow::Owned(
                starts
                    .par_iter()
                    .filter(|start| {
                        game_parallel_economy_work::tools::available(
                            &self.work_tools,
                            &view,
                            start.worker,
                            start.recipe,
                        )
                    })
                    .copied()
                    .collect(),
            )
        };
        let mut site_disregards = work
            .as_ref()
            .map_or_else(Vec::new, |plan| plan.site_disregards.clone());
        if !site_classes.is_empty() {
            site_disregards.extend(player_work.map(|choice| choice.actor()));
            site_disregards.par_sort_unstable();
            site_disregards.dedup();
        }
        let site_plan = self.work_sites.as_ref().map(|sites| {
            sites.prepare_access(
                &starts,
                &game_parallel_economy_work::sites::access::Access {
                    classes: &site_classes,
                    disregards: &site_disregards,
                },
                self.seed,
                tick,
            )
        });
        let starts = match (&self.work_sites, &site_plan) {
            (Some(sites), Some(plan)) => {
                std::borrow::Cow::Owned(sites.filter_starts(&starts, plan))
            }
            _ => starts,
        };
        let farm_harvest = farming.as_ref().map_or(&[][..], |p| p.harvest.as_slice());
        let gathered_harvest = gathering_plan
            .as_ref()
            .map_or(&[][..], |p| p.actions.as_slice());
        let combined_harvest: Option<Vec<_>> = (!gathered_harvest.is_empty()).then(|| {
            farm_harvest
                .par_iter()
                .chain(gathered_harvest.par_iter())
                .cloned()
                .collect()
        });
        let harvest = combined_harvest.as_deref().unwrap_or(farm_harvest);
        let mut physical = self.physical.prepare_with_growth(
            &rates,
            &actions,
            &starts,
            &abandon,
            game_parallel_lot_runtime::Growth {
                harvest,
                compact_consumption: &compact,
            },
            self.seed,
        )?;
        let fatigue = game_parallel_economy_fatigue::prepare_world!(
            self,
            (rates, starts, physical),
            (
                farming,
                gathering_plan,
                medical_plan.caregivers(),
                hauling_plan,
                care,
                fighters
            ),
            (resting, busy, meal_candidates, player_transfer)
        );
        actions.into_par_iter().for_each(drop);
        let given =
            manual_give.map(|(person, _, _, _)| giving_plan.delivered(person, physical.events()));
        if let Some(giving) = &mut next_giving {
            giving_plan.learn_sharing(giving, physical.events(), self.physical.goods());
            giving_plan.clear_fulfilled_requests(giving, physical.events(), self.physical.goods());
        }
        let gathered = manual_gather.map(|(person, _)| {
            gathering_plan
                .as_ref()
                .is_some_and(|p| p.admitted(person, physical.events()))
        });
        let mut gathering = gathering_plan
            .map(|p| {
                p.publish(
                    self.gathering.as_ref().expect("gathering owner"),
                    physical.events(),
                )
            })
            .transpose()?;
        let mut agriculture = game_parallel_economy_agriculture::publish_optional(
            farming,
            self.agriculture.as_ref(),
            physical.events(),
        )?;
        let effects = meals.effects(physical.events())?;
        let mut person = person::prepare(
            &self.people,
            &self.rules,
            self.nutrition.as_ref(),
            &self.pending_food,
            effects,
            tick,
        );
        let mut dynamics = game_parallel_economy_dynamics::prepare_dynamics!(
            self,
            person,
            busy,
            tick,
            treatment,
            physical,
            care,
            interaction,
            medical_plan
        );
        medical_plan.apply(
            dynamics.as_mut().and_then(|d| d.disease.as_mut()),
            physical.events(),
        );
        let treated_other = manual_medical.map(|(actor, patient, _, _)| {
            medical_plan.treated(
                actor,
                patient,
                dynamics.as_ref().and_then(|d| d.disease.as_ref()),
            )
        });
        let blocked = game_parallel_economy_goals::blocked(
            self.goals.as_ref(),
            self.autowork.as_ref(),
            work.as_ref(),
            &people,
            self.rules.max_work_hunger,
            physical.events(),
            self.physical.work(),
        );
        let mut purpose = purpose::prepare(
            self.purpose.as_ref(),
            self.physical.work(),
            physical.events(),
            &blocked,
            &mut person.people,
            tick,
        );
        let mut combat = family::stage_combat!(self, people, person, tick, view);
        game_parallel_economy_fatigue::apply(fatigue.as_ref(), &mut person.people);
        environment::apply(
            self.environment.as_ref(),
            &view,
            self.travel.as_ref(),
            &mut person.people,
            tick,
        );
        recovery::apply(
            self.recovery.as_ref(),
            &mut person.people,
            &resting,
            fighters,
            player_transfer,
            tick,
            self.rules.max_health,
        );
        let mut knowledge = game_parallel_economy_knowledge::prepare_world!(self, dynamics);
        let mut skills = game_parallel_economy_skills::advance(
            self.skills.as_ref(),
            self.physical.work(),
            physical.events(),
            dynamics.as_ref().and_then(|d| d.social.as_ref()),
            &self.people,
            tick,
        );
        let mut next_care = self.care.clone();
        if let (Some(plan), Some(care)) = (care, &mut next_care) {
            plan.apply(care, &mut person.people);
        }
        let mut travel = self
            .travel
            .as_ref()
            .map(|travel| travel.advance(&mut person.people));
        let mut food_search = self.food_search.clone();
        let alive_and_local = |d: &Departure| {
            person
                .people
                .binary_search_by_key(&d.person, |p| p.id)
                .ok()
                .is_some_and(|i| person.people[i].health > 0 && person.people[i].region.is_some())
        };
        let search_departures: Vec<_> = search_plan
            .as_ref()
            .map_or(&[][..], |plan| &plan.departures)
            .par_iter()
            .copied()
            .filter(alive_and_local)
            .collect();
        let mut departures: Vec<_> = hauling_plan
            .as_ref()
            .map_or(&[][..], |plan| &plan.departures)
            .par_iter()
            .copied()
            .filter(|d| {
                person
                    .people
                    .binary_search_by_key(&d.person, |p| p.id)
                    .ok()
                    .is_some_and(|i| {
                        person.people[i].health > 0 && person.people[i].region.is_some()
                    })
            })
            .collect();
        departures.par_extend(search_departures.par_iter().copied());
        if !departures.is_empty() {
            let current_travel = travel.as_mut().ok_or("departures require travel")?;
            let working = person::travel::working(self.physical.work());
            let (next_travel, next_people) =
                current_travel.depart(&person.people, &departures, &working)?;
            *current_travel = next_travel;
            person.people = next_people;
        }
        if let (Some(plan), Some(search)) = (search_plan, food_search.as_mut()) {
            search.publish(&search_departures, &plan.targets, &plan.abandoned, tick)?;
        }
        let (family, campaign) = family::advance_world!(
            self,
            person,
            travel,
            next_care,
            physical,
            dynamics,
            campaign_draft,
            tick,
            skills,
            knowledge
        );
        if let Some(travel) = &mut travel {
            travel.settle_deaths(&mut person.people);
        }
        purpose::enroll(purpose.as_mut(), family.as_ref(), tick)?;
        game_parallel_economy_agriculture::enroll(agriculture.as_mut(), family.as_ref(), tick);
        game_parallel_economy_claims::enroll(
            dynamics.as_mut().and_then(|owner| owner.claims.as_mut()),
            family.as_ref(),
            tick,
        );
        giving_inheritance::enroll(next_giving.as_mut(), family.as_ref(), tick);
        gathering::enroll(gathering.as_mut(), family.as_ref(), tick);
        food_search::enroll(food_search.as_mut(), family.as_ref(), tick);
        let deaths: Vec<_> = person
            .people
            .par_iter()
            .filter_map(|person| {
                (person.health == 0
                    && self
                        .people
                        .binary_search_by_key(&person.id, |p| p.id)
                        .ok()
                        .is_some_and(|i| self.people[i].health > 0))
                .then_some(person.id)
            })
            .collect();
        self.physical.interrupt_workers_in(&mut physical, &deaths)?;
        let mut work_sites = self
            .work_sites
            .as_ref()
            .zip(site_plan.as_ref())
            .map(|(sites, plan)| sites.advance(plan, physical.events()));
        let mut construction = self.construction.clone();
        if let (Some(plan), Some(owner), Some(sites)) = (
            construction_plan,
            construction.as_mut(),
            work_sites.as_mut(),
        ) {
            plan.publish(owner, sites, physical.events());
        } else if let Some(owner) = &mut construction {
            owner.events.clear();
        }
        let mut manually_destroyed = None;
        if let (Some((site, was_occupied)), Some(sites)) = (destroy_site, &mut work_sites)
            && !was_occupied
            && site_plan
                .as_ref()
                .is_none_or(|plan| !plan.reserves_site(site))
            && sites.destroy_idle(site)
            && let Some(game_parallel_economy_interaction::PhysicalChoice::DestroySite {
                person,
                ..
            }) = player_transfer
        {
            manually_destroyed = Some((person, site));
        }
        if let (Some(plan), Some(owner), Some(prior), Some(sites)) = (
            demolition_plan,
            construction.as_mut(),
            self.work_sites.as_ref(),
            work_sites.as_mut(),
        ) {
            owner.demolition_events = plan.publish(tick, prior, site_plan.as_ref(), sites);
        }
        if let Some(claims) = dynamics.as_mut().and_then(|d| d.claims.as_mut())
            && let Some(construction) = &construction
            && claims.preferences.par_iter().any(|p| p.assert_built_sites)
        {
            let built: Vec<_> = construction
                .events
                .par_iter()
                .filter_map(|event| event.site.map(|site| (event.person, site)))
                .collect();
            game_parallel_economy_claims::publish_built_site_claims(claims, &built, &person.people);
        }
        if let Some(owner) = dynamics.as_mut().and_then(|d| d.claims.as_mut()) {
            let mut uses = Vec::new();
            if !site_classes.is_empty() {
                let mut workers: Vec<_> = starts
                    .par_iter()
                    .map(|start| (start.id, start.worker))
                    .collect();
                workers.par_sort_unstable_by_key(|row| row.0);
                if let Some(plan) = &site_plan {
                    uses.par_extend(plan.bindings().par_iter().filter_map(|binding| {
                        if physical
                            .events()
                            .started
                            .binary_search(&binding.job)
                            .is_err()
                        {
                            return None;
                        }
                        let person = workers
                            .binary_search_by_key(&binding.job, |row| row.0)
                            .ok()
                            .map(|i| workers[i].1)?;
                        Some(game_parallel_economy_claims::SiteUse {
                            person,
                            site: binding.site,
                            destroyed: false,
                        })
                    }));
                }
                if let Some((person, site)) = manually_destroyed {
                    uses.push(game_parallel_economy_claims::SiteUse {
                        person,
                        site,
                        destroyed: true,
                    });
                }
                if let Some(construction) = &construction {
                    uses.par_extend(
                        construction
                            .demolition_events
                            .par_iter()
                            .filter(|e| e.destroyed)
                            .map(|e| game_parallel_economy_claims::SiteUse {
                                person: e.person,
                                site: e.site,
                                destroyed: true,
                            }),
                    );
                }
            }
            let sites: Vec<_> = self.work_sites.as_ref().map_or_else(Vec::new, |sites| {
                sites
                    .sites
                    .par_iter()
                    .map(|site| (site.id, site.region))
                    .collect()
            });
            game_parallel_economy_claims::publish_site_uses(
                owner,
                &uses,
                &sites,
                &self.people,
                tick,
            );
        }
        game_parallel_economy_combat::finish_world!(combat, person);
        let missions = game_parallel_economy_expeditions::finish_world!(
            self, expedition, person, travel, dynamics, tick
        );
        let mut next_market = self.market.clone();
        let mut next_opinions = game_parallel_economy_dynamics::next_opinions(&self.opinions);
        development::enroll(&mut next_opinions, family.as_ref(), tick);
        if let Some(claims) = dynamics
            .as_ref()
            .and_then(|d| d.claims.as_ref())
            .filter(|c| !c.site_events.is_empty())
            && let Some(opinions) = next_opinions.as_mut().filter(|o| o.tracks_site_affinity())
        {
            let witnessed: Vec<_> = claims
                .site_events
                .par_iter()
                .map(|e| (e.claimant, e.person, e.destroyed))
                .collect();
            opinions.learn_site_affinity(&witnessed);
        }
        if let Some(opinions) = next_opinions
            .as_mut()
            .filter(|o| !giving_plan.actions.is_empty() && o.tracks_help())
        {
            opinions.learn_help(&giving_plan.help(physical.events()));
        }
        if let Some(opinions) = next_opinions
            .as_mut()
            .filter(|o| !medical_plan.actions.is_empty() && o.tracks_help())
        {
            opinions.learn_help(&medical_plan.help(physical.events()));
        }
        game_parallel_economy_combat::learn_world!(combat, next_opinions);
        game_parallel_economy_exchange::publish(
            trade.as_ref(),
            proposals,
            &mut next_market,
            &mut next_opinions,
            physical.events(),
        )?;
        let mut next_agreements = self.agreements.clone();
        if let Some(agreements) = &mut next_agreements {
            agreement::advance(
                agreements,
                promises.delivery.as_ref(),
                &promises.decisions,
                physical.events(),
                &people,
                Some(&locations),
                next_opinions.as_mut(),
            )?;
        }
        if let Some(agreements) = &mut next_agreements {
            game_parallel_economy_promises::offers::publish(
                agreements,
                &promise_offers,
                tick,
                next_opinions.as_mut(),
            )?;
        }
        game_parallel_economy_dynamics::learn_social(&mut next_opinions, dynamics.as_ref(), tick);
        game_parallel_economy_claims::publish_claims!(
            dynamics,
            claim_uses,
            physical.events(),
            self.people,
            next_opinions
        );
        game_parallel_economy_claims_output::publish!(dynamics, claim_outputs, physical.events());
        development::advance(
            &mut next_opinions,
            next_care.as_ref(),
            self.family.as_ref(),
            tick,
        );
        agreement::decay(next_opinions.as_mut());
        let mut autowork = self.autowork.clone();
        if let (Some(auto), Some(plan)) = (&mut autowork, work) {
            plan.publish(auto, physical.events(), &people);
        }
        if let Some((requests, family)) = autowork
            .as_mut()
            .and_then(|auto| auto.requests.as_mut())
            .zip(family.as_ref())
        {
            requests.enroll(family, tick);
        }
        game_parallel_economy_interaction::publish_work!(autowork, manual_job);
        let (goals, dreams) = game_parallel_economy_goals::advance(
            self.goals.as_ref(),
            self.dreams.as_ref(),
            self.physical.work(),
            physical.events(),
            &person.people,
            &blocked,
        )?;
        Ok(Draft {
            epoch: self.epoch,
            physical,
            people: person.people,
            market: next_market,
            agreements: next_agreements,
            opinions: next_opinions,
            autowork,
            work_sites,
            construction,
            goals,
            dreams,
            nutrition: person.nutrition,
            pending_food: person.pending,
            travel,
            family,
            care: next_care,
            agriculture,
            dynamics,
            missions,
            campaign,
            skills,
            knowledge,
            combat,
            fatigue,
            purpose,
            gathering,
            gathered,
            food_search,
            giving: next_giving,
            hauling: next_hauling,
            given,
            treated_other,
        })
    }
    pub fn commit(&mut self, draft: Draft) -> Result<Events, String> {
        if draft.epoch != self.epoch {
            return Err("stale economic draft".into());
        }
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("economic generation overflow")?;
        self.physical.validate_advance(&draft.physical)?;
        let events = self.physical.commit_advance(draft.physical)?;
        self.people = draft.people;
        self.market = draft.market;
        self.agreements = draft.agreements;
        self.opinions = draft.opinions;
        self.autowork = draft.autowork;
        self.work_sites = draft.work_sites;
        self.construction = draft.construction;
        self.goals = draft.goals;
        self.dreams = draft.dreams;
        self.nutrition = draft.nutrition;
        self.pending_food = draft.pending_food;
        self.travel = draft.travel;
        self.family = draft.family;
        self.care = draft.care;
        self.agriculture = draft.agriculture;
        self.dynamics = draft.dynamics;
        self.missions = draft.missions;
        self.campaign = draft.campaign;
        self.skills = draft.skills;
        self.knowledge = draft.knowledge;
        game_parallel_economy_combat::publish_world!(self, draft.combat);
        self.fatigue = draft.fatigue.map(|fatigue| fatigue.state);
        self.purpose = draft.purpose;
        self.gathering = draft.gathering;
        self.food_search = draft.food_search;
        self.giving = draft.giving;
        self.hauling = draft.hauling;
        self.epoch = epoch;
        Ok(events)
    }
    pub fn advance(&mut self) -> Result<Events, String> {
        game_parallel_economy_campaign::preflight!(self);
        let draft = self.prepare()?;
        self.commit(draft)
    }
}
#[cfg(test)]
#[path = "../tests/support/behavior.rs"]
mod tests;
