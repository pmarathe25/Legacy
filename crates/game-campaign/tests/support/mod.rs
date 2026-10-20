#![allow(dead_code)]
use game_core::*;
use game_sim::*;
use std::collections::{BTreeMap, BTreeSet};
pub fn builder() -> Builder {
    let mut b = Builder::new();
    game_land::plugin(&mut b).unwrap();
    game_transport::plugin(&mut b).unwrap();
    game_vitality::plugin(&mut b).unwrap();
    game_objects::plugin(&mut b).unwrap();
    game_traits::plugin(&mut b).unwrap();
    game_cargo::plugin(&mut b).unwrap();
    game_property::plugin(&mut b).unwrap();
    game_environment::plugin(&mut b).unwrap();
    game_physiology::plugin(&mut b).unwrap();
    game_communication::plugin(&mut b).unwrap();
    game_knowledge::plugin(&mut b).unwrap();
    game_testimony::plugin(&mut b).unwrap();
    game_search::plugin(&mut b).unwrap();
    game_skills::plugin(&mut b).unwrap();
    game_ecology::plugin(&mut b).unwrap();
    game_agriculture::plugin(&mut b).unwrap();
    game_workshop::plugin(&mut b).unwrap();
    game_health::plugin(&mut b).unwrap();
    game_relationships::plugin(&mut b).unwrap();
    game_agreements::plugin(&mut b).unwrap();
    game_institutions::plugin(&mut b).unwrap();
    game_lifecycle::plugin(&mut b).unwrap();
    game_lineage::plugin(&mut b).unwrap();
    game_guardians::plugin(&mut b).unwrap();
    game_invocation::plugin(&mut b).unwrap();
    game_social_life::plugin(&mut b).unwrap();
    game_characters::plugin(&mut b).unwrap();
    game_conflict::plugin(&mut b).unwrap();
    game_pressure::plugin(&mut b).unwrap();
    game_communities::plugin(&mut b).unwrap();
    game_stockpiles::plugin(&mut b).unwrap();
    game_wayfaring::plugin(&mut b).unwrap();
    game_horticulture::plugin(&mut b).unwrap();
    game_care::plugin(&mut b).unwrap();
    game_livelihoods::plugin(&mut b).unwrap();
    game_pastimes::plugin(&mut b).unwrap();
    game_heredity::plugin(&mut b).unwrap();
    game_households::plugin(&mut b).unwrap();
    game_family_life::plugin(&mut b).unwrap();
    game_parenthood::plugin(&mut b).unwrap();
    game_exchange::plugin(&mut b).unwrap();
    game_works::plugin(&mut b).unwrap();
    game_preservation::plugin(&mut b).unwrap();
    game_equipment::plugin(&mut b).unwrap();
    game_comfort::plugin(&mut b).unwrap();
    game_grievances::plugin(&mut b).unwrap();
    game_affiliation::plugin(&mut b).unwrap();
    b
}
pub fn world() -> (Host, AreaId) {
    world_with_spread(u16::MAX)
}
pub fn world_with_spread(intelligence_spread: u16) -> (Host, AreaId) {
    let mut h = builder().build(9, 1).unwrap();
    h.advance(SimTime(50 * 365 * DAY.0), 1).unwrap();
    h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    let mut traits = game_traits::load(&mut game_content::Store::standard()).unwrap();
    traits
        .definitions
        .get_mut("intelligence")
        .unwrap()
        .founder_spread = intelligence_spread;
    if intelligence_spread != u16::MAX {
        traits.stamps.clear();
    }
    h.dispatch(game_traits::Install(traits)).unwrap();
    h.dispatch(game_environment::Configure(game_environment::Climate {
        seasons: vec![game_environment::Season {
            duration: DAY,
            warmth: 10,
            rain: 20,
        }],
    }))
    .unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "Clearing".into(),
        })
        .unwrap();
    (h, area)
}
pub fn person(h: &mut Host, area: AreaId) -> (PersonId, InventoryId) {
    let person = h
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: SimTime(0),
        })
        .unwrap();
    h.dispatch(game_transport::Place { person, area }).unwrap();
    h.dispatch(game_social_life::Register {
        person,
        name: format!("Person {}", person.0),
    })
    .unwrap();
    h.dispatch(game_traits::Set {
        person,
        profile: BTreeMap::from([
            ("empathy".into(), 900),
            ("patience".into(), 900),
            ("intelligence".into(), 12000),
            ("learning".into(), 4000),
        ]),
    })
    .unwrap();
    h.dispatch(game_relationships::Initialize {
        person,
        beliefs: Default::default(),
        values: Default::default(),
    })
    .unwrap();
    let inventory = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(person),
            capacity: 2000,
        })
        .unwrap();
    h.dispatch(game_property::ClaimInventory {
        claimant: person,
        inventory,
        recognized_by: BTreeSet::from([person]),
    })
    .unwrap();
    h.dispatch(game_property::Permit {
        issuer: person,
        person,
        inventory,
        allow: true,
    })
    .unwrap();
    (person, inventory)
}

pub fn set_opinion(h: &mut Host, person: PersonId, concept: &str, target: i32) {
    loop {
        let current = h
            .get::<game_relationships::Social>()
            .unwrap()
            .opinion(person, concept);
        if current == target {
            break;
        }
        h.dispatch(game_relationships::ReviseOpinion {
            person,
            concept: concept.into(),
            change: target - current,
        })
        .unwrap();
    }
}
