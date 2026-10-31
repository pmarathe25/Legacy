//! Borrowed committed owners for callers constructing restricted observations.
use game_parallel_agreements::Agreements;
use game_parallel_autowork::Autowork;
use game_parallel_economy_agriculture::Agriculture;
use game_parallel_economy_campaign::Campaign;
use game_parallel_economy_combat::Combat;
use game_parallel_economy_dynamics::Dynamics;
use game_parallel_economy_expeditions::Missions;
use game_parallel_economy_fatigue::Fatigue;
use game_parallel_economy_knowledge::Knowledge;
use game_parallel_economy_person::Person;
use game_parallel_economy_person::travel::Travel;
use game_parallel_environment::Environment;
use game_parallel_lot_runtime::Runtime;
use game_parallel_purpose::Purpose;

pub struct Read<'a> {
    pub tick: u64,
    pub people: &'a [Person],
    pub physical: &'a Runtime,
    pub campaign: Option<&'a Campaign>,
    pub knowledge: Option<&'a Knowledge>,
    pub missions: Option<&'a Missions>,
    pub agreements: Option<&'a Agreements>,
    pub combat: Option<&'a Combat>,
    pub autowork: Option<&'a Autowork>,
    pub work_sites: Option<&'a game_parallel_economy_work::sites::Facilities>,
    pub work_tools: &'a [game_parallel_economy_work::tools::Requirement],
    pub dynamics: Option<&'a Dynamics>,
    pub agriculture: Option<&'a Agriculture>,
    pub fatigue: Option<&'a Fatigue>,
    pub environment: Option<&'a Environment>,
    pub purpose: Option<&'a Purpose>,
    pub travel: Option<&'a Travel>,
}
#[macro_export]
macro_rules! read_method {
    () => {
        /// Administrative borrowed access; player-facing views must filter these owners.
        pub fn read(&self) -> $crate::Read<'_> {
            $crate::Read {
                tick: self.tick(),
                people: &self.people,
                physical: &self.physical,
                campaign: self.campaign.as_ref(),
                knowledge: self.knowledge.as_ref(),
                missions: self.missions.as_ref(),
                agreements: self.agreements.as_ref(),
                combat: self.combat.as_ref().map(|combat| &combat.state),
                autowork: self.autowork.as_ref(),
                work_sites: self.work_sites.as_ref(),
                work_tools: &self.work_tools,
                dynamics: self.dynamics.as_ref(),
                agriculture: self.agriculture.as_ref(),
                fatigue: self.fatigue.as_ref(),
                environment: self.environment.as_ref(),
                purpose: self.purpose.as_ref(),
                travel: self.travel.as_ref(),
            }
        }
    };
}
