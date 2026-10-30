use game_parallel::{Move, World};
use std::collections::{BTreeMap, BTreeSet};

fn world() -> World {
    World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap()
}
fn people(w: &World) -> BTreeMap<u64, u32> {
    w.regions
        .iter()
        .flat_map(|r| r.people.iter().map(|p| (p.id, p.health)))
        .chain(w.travelers.iter().map(|t| (t.person.id, t.person.health)))
        .collect()
}
fn canonical(mut w: World) -> World {
    w.regions.sort_by_key(|r| r.id);
    for r in &mut w.regions {
        r.people.sort_by_key(|p| p.id);
    }
    w
}
#[test]
fn generations_are_worker_reload_and_layout_independent_with_population_conservation() {
    let initial = world();
    let mut worlds = Vec::new();
    for workers in [1, 4, 8] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let mut w = initial.clone();
        if workers > 1 {
            w.regions.reverse();
            for r in &mut w.regions {
                r.people.reverse();
            }
            w.families.as_mut().unwrap().members.reverse();
        }
        pool.install(|| {
            for tick in 1..=100 {
                let living = people(&w).values().filter(|h| **h > 0).count() as i64;
                let before = people(&w).len();
                w.step().unwrap();
                w.validate().unwrap();
                let f = w.families.as_ref().unwrap();
                let born = f.events.iter().filter(|e| e.kind == "birth").count();
                let died = f.events.iter().filter(|e| e.kind == "death").count();
                assert_eq!(people(&w).len(), before + born);
                assert_eq!(
                    people(&w).values().filter(|h| **h > 0).count() as i64,
                    living + born as i64 - died as i64
                );
                assert_eq!(f.members.len(), 8 + f.totals.births as usize);
                if tick == 2 || tick == 30 {
                    w = World::from_json(&serde_json::to_string(&w).unwrap()).unwrap();
                }
            }
        });
        let f = w.families.as_ref().unwrap();
        assert!(f.totals.births > 8 && f.totals.deaths >= 8 && f.totals.adulthoods > 0);
        assert!(f.members.iter().any(|m| m.parents.iter().any(|p| {
            f.members
                .iter()
                .find(|a| a.id == *p)
                .is_some_and(|a| !a.parents.is_empty())
        })));
        worlds.push(canonical(w));
    }
    assert!(worlds.windows(2).all(|w| w[0] == w[1]));
}
#[test]
fn maternal_death_loses_pregnancy_but_paternal_death_preserves_ancestry() {
    for kill_mother in [true, false] {
        let mut w = world();
        w.step().unwrap();
        let mother = w
            .families
            .as_ref()
            .unwrap()
            .members
            .iter()
            .find(|m| m.pregnancy.is_some())
            .unwrap()
            .clone();
        let father = mother.pregnancy.as_ref().unwrap().father;
        let dead = if kill_mother { mother.id } else { father };
        for r in &mut w.regions {
            if let Some(p) = r.people.iter_mut().find(|p| p.id == dead) {
                p.health = 0;
            }
        }
        w.step().unwrap();
        assert_eq!(w.families.as_ref().unwrap().totals.deaths, 1);
        assert_eq!(
            w.families.as_ref().unwrap().totals.pregnancy_losses,
            u64::from(kill_mother)
        );
        w = World::from_json(&serde_json::to_string(&w).unwrap()).unwrap();
        while w.tick < 4 {
            w.step().unwrap();
        }
        let descendants = w
            .families
            .as_ref()
            .unwrap()
            .members
            .iter()
            .filter(|m| m.parents.as_slice() == [mother.id, father])
            .count();
        assert_eq!(descendants, usize::from(!kill_mother));
        w.validate().unwrap();
    }
}
#[test]
fn birth_during_travel_preserves_one_identity_and_arrives_with_its_mother() {
    let mut w = world();
    w.step().unwrap();
    let mother = w
        .families
        .as_ref()
        .unwrap()
        .members
        .iter()
        .find(|m| m.id <= 4 && m.pregnancy.is_some())
        .unwrap()
        .id;
    w.move_people(&[Move {
        person: mother,
        destination: 2,
    }])
    .unwrap();
    while w.tick < 4 {
        w.step().unwrap();
    }
    let child = w
        .families
        .as_ref()
        .unwrap()
        .members
        .iter()
        .find(|m| m.parents.first() == Some(&mother))
        .unwrap()
        .id;
    assert!(w.travelers.iter().any(|t| t.person.id == child));
    assert!(w.travelers.iter().any(|t| t.person.id == mother));
    let mut w = World::from_json(&serde_json::to_string(&w).unwrap()).unwrap();
    while w.tick < 6 {
        w.step().unwrap();
    }
    let arrived = &w.regions.iter().find(|r| r.id == 2).unwrap().people;
    assert!(arrived.iter().any(|p| p.id == mother) && arrived.iter().any(|p| p.id == child));
    assert_eq!(people(&w).len(), w.families.as_ref().unwrap().members.len());
    w.validate().unwrap();
}
#[test]
fn conception_requires_opt_in_and_inheritance_uses_both_parental_alleles() {
    let mut w = world();
    w.families.as_mut().unwrap().rules.mutation_per_million = 0;
    w.families.as_mut().unwrap().members[0].reproduction_allowed = false;
    for _ in 0..4 {
        w.step().unwrap();
    }
    let f = w.families.as_ref().unwrap();
    assert!(f.members.iter().all(|m| !m.parents.contains(&1)));
    assert_eq!(f.members[0].partner, None);
    let index: BTreeMap<_, _> = f.members.iter().map(|m| (m.id, m)).collect();
    let children: Vec<_> = f.members.iter().filter(|m| !m.parents.is_empty()).collect();
    assert!(!children.is_empty());
    for child in children {
        for locus in 0..4 {
            for side in 0..2 {
                assert!(
                    index[&child.parents[side]].genome.alleles[locus]
                        .contains(&child.genome.alleles[locus][side])
                );
            }
        }
    }
}
#[test]
fn fallible_birth_phase_rolls_back_food_age_travel_and_all_family_effects() {
    let mut w = world();
    for _ in 0..3 {
        w.step().unwrap();
    }
    w.families.as_mut().unwrap().next_id = u64::MAX;
    let before = w.clone();
    assert!(w.step().is_err());
    assert_eq!(w, before);
}
#[test]
fn newborns_do_not_create_food_and_known_close_kin_never_pair() {
    let mut w = world();
    for _ in 0..3 {
        w.step().unwrap();
    }
    let mut physical = w.clone();
    physical.families = None;
    physical.step().unwrap();
    w.step().unwrap();
    assert!(
        w.families
            .as_ref()
            .unwrap()
            .events
            .iter()
            .any(|e| e.kind == "birth")
    );
    assert_eq!(
        w.regions.iter().map(|r| r.food).collect::<Vec<_>>(),
        physical.regions.iter().map(|r| r.food).collect::<Vec<_>>()
    );
    for _ in 0..40 {
        w.step().unwrap();
        let f = w.families.as_ref().unwrap();
        let index: BTreeMap<_, _> = f.members.iter().map(|m| (m.id, m)).collect();
        let ancestors = |id: u64| {
            let mut a = BTreeSet::from([id]);
            for p in &index[&id].parents {
                a.insert(*p);
                a.extend(index[p].parents.iter().copied());
            }
            a
        };
        for m in &f.members {
            if let Some(p) = m.partner {
                assert!(ancestors(m.id).is_disjoint(&ancestors(p)));
            }
        }
    }
}

#[test]
fn mutation_is_bounded_and_bad_genomes_or_stale_family_clocks_are_rejected() {
    let mut w = world();
    let f = w.families.as_mut().unwrap();
    f.rules.mutation_per_million = 1_000_000;
    f.rules.mutation_step = 10;
    for m in &mut f.members {
        m.genome.alleles = [[500; 2]; 4];
    }
    for _ in 0..8 {
        w.step().unwrap();
    }
    let children: Vec<_> = w
        .families
        .as_ref()
        .unwrap()
        .members
        .iter()
        .filter(|m| !m.parents.is_empty())
        .collect();
    assert!(!children.is_empty());
    for child in children {
        assert!(
            child
                .genome
                .alleles
                .iter()
                .flatten()
                .all(|a| [490, 510].contains(a))
        );
    }
    let mut malformed = w.clone();
    malformed.families.as_mut().unwrap().members[0]
        .genome
        .alleles[0][0] = 1001;
    assert!(World::from_json(&serde_json::to_string(&malformed).unwrap()).is_err());
    let mut malformed = w;
    malformed.families.as_mut().unwrap().last_tick -= 1;
    assert!(World::from_json(&serde_json::to_string(&malformed).unwrap()).is_err());
}

#[test]
fn family_identity_join_rejects_duplicate_and_missing_physical_rows() {
    let initial = world();
    let mut duplicated = initial.clone();
    duplicated.regions[0].people[0].id = duplicated.regions[0].people[1].id;
    assert!(duplicated.validate().is_err());
    let before = duplicated.clone();
    assert!(duplicated.step().is_err());
    assert_eq!(duplicated, before);

    let mut missing = initial.clone();
    missing.regions[0].people.pop();
    assert!(missing.validate().is_err());
    let before = missing.clone();
    assert!(missing.step().is_err());
    assert_eq!(missing, before);

    let mut extra = initial;
    let mut person = extra.regions[0].people[0].clone();
    person.id = 999;
    extra.regions[0].people.push(person);
    assert!(extra.validate().is_err());
    let before = extra.clone();
    assert!(extra.step().is_err());
    assert_eq!(extra, before);
}

#[test]
fn family_identity_join_rejects_mismatched_and_duplicate_members() {
    let mut mismatched = world();
    mismatched.regions[0].people[0].id = 999;
    assert!(mismatched.validate().is_err());
    let before = mismatched.clone();
    assert!(mismatched.step().is_err());
    assert_eq!(mismatched, before);
    let mut duplicate = world();
    let f = duplicate.families.as_mut().unwrap();
    f.members[0].id = f.members[1].id;
    assert!(duplicate.validate().is_err());
    let before = duplicate.clone();
    assert!(duplicate.step().is_err());
    assert_eq!(duplicate, before);
}

#[test]
fn failed_family_step_does_not_publish_shipment_or_travel_progress() {
    use game_parallel::Shipment;
    let mut w = world();
    w.move_people(&[Move {
        person: 1,
        destination: 2,
    }])
    .unwrap();
    w.launch_shipments(&[Shipment {
        id: 20,
        source: 2,
        destination: 1,
        food: 3,
        ticks_remaining: 5,
    }])
    .unwrap();
    w.families
        .as_mut()
        .unwrap()
        .members
        .iter_mut()
        .find(|m| m.id == 2)
        .unwrap()
        .age = u64::MAX;
    let before = w.clone();
    assert!(w.step().is_err());
    assert_eq!(w, before);
}
