//! A live invocation lets the player consult records from its chosen lives.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CorePastLifeView {
    pub person: u64,
    pub known_recipes: Vec<u32>,
    pub food_reports: Vec<game_economy_knowledge::Record>,
    pub route_reports: Vec<game_economy_person::travel::RouteObservation>,
    pub words: Vec<game_language::Word>,
    pub written_forms: Vec<game_language::writing::Association>,
}

impl CoreHost {
    pub fn consult_past_life(&self, person: u64, life: u64) -> Result<CorePastLifeView, String> {
        let traveler = self.controlled_person(person)?;
        let owner = self
            .campaign
            .as_ref()
            .filter(|campaign| campaign.controlled() == Some(person))
            .and_then(|campaign| campaign.costs.as_ref())
            .filter(|clock| clock.owner.active.contains(&life))
            .ok_or("consulting a past life requires its active invocation")?;
        if traveler.health == 0 || owner.owner.owner != Some(person) {
            return Err("the current incarnation cannot consult this past life".into());
        }
        let known_recipes = self
            .skills
            .as_ref()
            .and_then(|skills| skills.owner.recipe_knowledge.as_ref())
            .and_then(|knowledge| {
                knowledge
                    .minds
                    .binary_search_by_key(&life, |row| row.person)
                    .ok()
                    .map(|index| knowledge.minds[index].known.clone())
            })
            .unwrap_or_default();
        let food_reports = self
            .knowledge
            .as_ref()
            .and_then(|knowledge| {
                knowledge
                    .minds
                    .binary_search_by_key(&life, |row| row.person)
                    .ok()
                    .map(|index| knowledge.minds[index].records.clone())
            })
            .unwrap_or_default();
        let route_reports = self.core.travel.as_ref().map_or_else(Vec::new, |travel| {
            travel
                .observations
                .par_iter()
                .filter(|row| row.person == life)
                .cloned()
                .collect()
        });
        let language = self
            .social
            .as_ref()
            .and_then(|owner| owner.language.as_ref());
        let words = language
            .and_then(|language| {
                language
                    .minds
                    .binary_search_by_key(&life, |row| row.person)
                    .ok()
                    .map(|index| language.minds[index].words.clone())
            })
            .unwrap_or_default();
        let written_forms = language
            .and_then(|language| language.writing.as_ref())
            .and_then(|forms| {
                forms
                    .minds
                    .binary_search_by_key(&life, |row| row.person)
                    .ok()
                    .map(|index| forms.minds[index].associations.clone())
            })
            .unwrap_or_default();
        Ok(CorePastLifeView {
            person: life,
            known_recipes,
            food_reports,
            route_reports,
            words,
            written_forms,
        })
    }
}
