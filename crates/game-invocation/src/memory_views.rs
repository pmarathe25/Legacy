//! Record visibility and offered read actions belong to their content owner.
use crate::*;
use game_core::*;
use game_ecs::Store;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryRecordView {
    pub id: RecordId,
    pub inventory: InventoryId,
    pub content: Option<RecordedMemory>,
    pub written_at: SimTime,
    pub author: PersonId,
    pub position: StockPosition,
    pub can_read: bool,
}
impl Runtime {
    pub fn record_views(
        &self,
        store: &Store,
        facts: &impl MemoryFacts,
        actor: PersonId,
    ) -> Vec<MemoryRecordView> {
        let Some(person) = facts.person(actor, actor) else {
            return Vec::new();
        };
        records(store)
            .filter_map(|(id, record)| {
                let stock = game_cargo::inventory(store, record.inventory)?;
                let visible = match (&stock.location, &person.location) {
                    (game_cargo::InventoryLocation::Carried(holder), _) => *holder == actor,
                    (game_cargo::InventoryLocation::Area(area), Location::AtArea(here)) => {
                        area == here
                    }
                    _ => false,
                };
                if !visible || !record.intact {
                    return None;
                }
                Some(MemoryRecordView {
                    id,
                    inventory: record.inventory,
                    content: record
                        .readers
                        .contains(&actor)
                        .then(|| record.content.clone()),
                    written_at: record.written_at,
                    author: record.author,
                    position: match stock.location {
                        game_cargo::InventoryLocation::Area(area) => StockPosition::Area(area),
                        game_cargo::InventoryLocation::Carried(person) => {
                            StockPosition::Carried(person)
                        }
                    },
                    can_read: self.can_read_record(store, facts, actor, id),
                })
            })
            .take(64)
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvokedLifeView {
    pub person: PersonId,
    pub died_at: SimTime,
    pub skills: Vec<SkillView>,
    pub stocks: Vec<StockKnowledge>,
    pub routes: Vec<(AreaId, AreaId, SimDuration)>,
    pub institutions: Vec<game_society::institutions::InstitutionView>,
    pub cases: Vec<game_society::institutions::SocialCaseView>,
}
impl Runtime {
    pub fn available_lives<A: game_ecs::Component>(&self, store: &Store) -> Vec<PersonId> {
        archives::<A>(store).map(|(id, _)| id).collect()
    }
    pub fn invoked_life_views<A: game_ecs::Component>(
        &self,
        store: &Store,
        actor: PersonId,
    ) -> Vec<InvokedLifeView> {
        self.control
            .active(store)
            .filter(|active| active.actor == actor)
            .into_iter()
            .flat_map(|active| &active.lives)
            .filter_map(|id| {
                let life = archive::<A>(store, *id)?;
                Some(InvokedLifeView {
                    person: *id,
                    died_at: life.died_at,
                    skills: life.skills.clone(),
                    stocks: life.stocks.known.values().take(64).cloned().collect(),
                    routes: life
                        .mind
                        .routes
                        .iter()
                        .flat_map(|(from, links)| {
                            links.iter().map(|(to, duration)| (*from, *to, *duration))
                        })
                        .take(128)
                        .collect(),
                    institutions: life.institutions.clone(),
                    cases: life.cases.clone(),
                })
            })
            .collect()
    }
}
