use game_families::{Families, FamilyRules, Genome, LifeInput, MatePreference, Member, Sex};
use game_family_model::{MateFeelings, PhysicalView};

struct View {
    reverse_affinity: i32,
    hunger: u32,
}
impl PhysicalView for View {
    fn read(&self, id: u64) -> LifeInput {
        LifeInput {
            id,
            region: Some(1),
            health: 100,
            hunger: if id == 1 { self.hunger } else { 0 },
        }
    }
    fn affinity(&self, observer: u64, target: u64) -> i32 {
        match (observer, target) {
            (1, 3) => 10,
            (3, 1) => self.reverse_affinity,
            (1, 2) | (2, 1) => 5,
            (4, 3) => 10,
            (3, 4) => 15,
            _ => -10,
        }
    }
}
struct PressureView(View);
impl PhysicalView for PressureView {
    fn read(&self, id: u64) -> LifeInput {
        self.0.read(id)
    }
    fn mate_feelings(&self, observer: u64, target: u64) -> MateFeelings {
        if (observer, target) == (1, 3) {
            MateFeelings {
                affinity: -10,
                trust: 20,
                fear: 20,
                obligation: 20,
            }
        } else {
            MateFeelings {
                affinity: self.0.affinity(observer, target),
                ..MateFeelings::default()
            }
        }
    }
}
struct RespondentView(View);
impl PhysicalView for RespondentView {
    fn read(&self, id: u64) -> LifeInput {
        self.0.read(id)
    }
    fn mate_feelings(&self, observer: u64, target: u64) -> MateFeelings {
        if (observer, target) == (3, 1) {
            MateFeelings {
                affinity: -10,
                fear: 20,
                ..MateFeelings::default()
            }
        } else {
            MateFeelings {
                affinity: self.0.affinity(observer, target),
                ..MateFeelings::default()
            }
        }
    }
}
fn pref() -> MatePreference {
    MatePreference {
        desire: 1000,
        genetic_weight: 0,
        affinity_weight: 1,
        trust_weight: 0,
        fear_weight: 0,
        obligation_weight: 0,
        attraction_weight: 0,
        female_attraction: 0,
        male_attraction: 0,
        hunger_weight: 0,
        dependent_weight: 0,
        minimum: 1,
    }
}
fn member(id: u64, sex: Sex, partner: Option<u64>) -> Member {
    Member {
        id,
        born: None,
        age: 20,
        lifespan_loss: 0,
        genome: Genome {
            sex,
            alleles: [[1000; 2]; 4],
            mate_desire: [500; 2],
        },
        parents: Default::default(),
        partner,
        pregnancy: None,
        next_conception: 0,
        alive: true,
        reproduction_allowed: true,
        reconsider_partnership: false,
        mate_preference: Some(pref()),
    }
}
fn family() -> Families {
    Families {
        rules: FamilyRules {
            adult_age: 16,
            min_work_age: 0,
            fertile_until: 45,
            gestation: 1,
            birth_spacing: 4,
            lifespan_min: 65,
            lifespan_max: 85,
            conception_one_in: 1,
            minimum_health: 1,
            maximum_hunger: 100,
            newborn_health: 100,
            mutation_per_million: 0,
            mutation_step: 0,
            enroll_births: false,
            subjective_mating: true,
        },
        members: vec![
            member(1, Sex::Female, Some(2)),
            member(2, Sex::Male, Some(1)),
            member(3, Sex::Male, None),
        ],
        next_id: 4,
        last_tick: 0,
        totals: Default::default(),
        events: vec![],
    }
}
#[test]
fn each_persons_affinity_matters_and_partnership_does_not_restrict_conception() {
    let state = family();
    let view = View {
        reverse_affinity: 10,
        hunger: 0,
    };
    let result = state.advance_validated(&view, 9, 1).unwrap();
    assert_eq!(
        result.families.members[0]
            .pregnancy
            .as_ref()
            .unwrap()
            .father,
        3
    );
    assert_eq!(result.families.members[0].partner, Some(2));
    let rejected = state
        .advance_validated(
            &View {
                reverse_affinity: -10,
                hunger: 0,
            },
            9,
            1,
        )
        .unwrap();
    assert_eq!(
        rejected.families.members[0]
            .pregnancy
            .as_ref()
            .unwrap()
            .father,
        2
    );
}
#[test]
fn fear_trust_and_obligation_have_individual_signed_effects() {
    let mut state = family();
    let view = PressureView(View {
        reverse_affinity: 10,
        hunger: 0,
    });
    let father = |state: &Families| {
        state
            .advance_validated(&view, 9, 1)
            .unwrap()
            .families
            .members[0]
            .pregnancy
            .as_ref()
            .unwrap()
            .father
    };
    assert_eq!(father(&state), 2);
    state.members[0]
        .mate_preference
        .as_mut()
        .unwrap()
        .fear_weight = 1;
    assert_eq!(father(&state), 3);
    let pref = state.members[0].mate_preference.as_mut().unwrap();
    pref.fear_weight = -1;
    pref.obligation_weight = 1;
    assert_eq!(father(&state), 2);
    let pref = state.members[0].mate_preference.as_mut().unwrap();
    pref.fear_weight = 0;
    pref.obligation_weight = 0;
    pref.trust_weight = 1;
    assert_eq!(father(&state), 3);
}
#[test]
fn a_person_can_respond_to_fear_without_actively_seeking_a_mate() {
    let mut state = family();
    let view = RespondentView(View {
        reverse_affinity: 10,
        hunger: 0,
    });
    let preference = state.members[2].mate_preference.as_mut().unwrap();
    preference.desire = 0;
    preference.fear_weight = 1;
    let result = state.advance_validated(&view, 9, 1).unwrap().families;
    assert_eq!(result.members[0].pregnancy.as_ref().unwrap().father, 3);
    state.members[0].partner = None;
    state.members[1].partner = None;
    let result = state.advance_validated(&view, 9, 1).unwrap().families;
    assert_eq!(result.members[0].partner, Some(3));
    state.members[2]
        .mate_preference
        .as_mut()
        .unwrap()
        .fear_weight = 0;
    let result = state.advance_validated(&view, 9, 1).unwrap().families;
    assert_eq!(result.members[0].pregnancy.as_ref().unwrap().father, 2);
}
#[test]
fn people_choose_partnerships_subjectively_and_either_can_leave() {
    let mut state = family();
    state.members[0].partner = None;
    state.members[1].partner = None;
    let view = View {
        reverse_affinity: 10,
        hunger: 0,
    };
    let partnered = state.advance_validated(&view, 9, 1).unwrap().families;
    assert_eq!(partnered.members[0].partner, Some(3));
    assert_eq!(partnered.members[2].partner, Some(1));
    let next = partnered
        .advance_validated(
            &View {
                reverse_affinity: -10,
                hunger: 0,
            },
            9,
            2,
        )
        .unwrap()
        .families;
    assert_ne!(next.members[0].partner, Some(3));
    assert!(
        next.events
            .iter()
            .any(|event| event.kind == "separation" && event.people == [1, 3])
    );
}
#[test]
fn recipient_chooses_their_preferred_acceptable_suitor() {
    let mut state = family();
    state.members[0].partner = None;
    state.members[1].partner = None;
    state.members[1].reproduction_allowed = false;
    state.members.push(member(4, Sex::Female, None));
    state.next_id = 5;
    let next = state
        .advance_validated(
            &View {
                reverse_affinity: 10,
                hunger: 0,
            },
            9,
            1,
        )
        .unwrap()
        .families;
    assert_eq!(next.members[2].partner, Some(4));
    assert_eq!(next.members[3].partner, Some(3));
    assert_eq!(next.members[0].partner, None);
}

#[test]
fn partnership_does_not_require_fertility_or_a_wish_to_reproduce() {
    let mut state = family();
    state.members[0].partner = None;
    state.members[1].partner = None;
    state.members[0].age = 50;
    state.members[2].age = 50;
    state.members[0].reproduction_allowed = false;
    state.members[2].reproduction_allowed = false;
    let result = state
        .advance_validated(
            &View {
                reverse_affinity: 10,
                hunger: 0,
            },
            9,
            1,
        )
        .unwrap()
        .families;
    assert_eq!(result.members[0].partner, Some(3));
    assert!(result.members[0].pregnancy.is_none());
}

#[test]
fn same_sex_partnerships_and_individual_attraction_preferences_are_possible() {
    let mut state = family();
    state.members.truncate(2);
    state.next_id = 3;
    for person in &mut state.members {
        person.partner = None;
        person.genome.sex = Sex::Male;
    }
    let view = View {
        reverse_affinity: 10,
        hunger: 0,
    };
    let paired = state.advance_validated(&view, 9, 1).unwrap().families;
    assert_eq!(paired.members[0].partner, Some(2));
    state.members[0]
        .mate_preference
        .as_mut()
        .unwrap()
        .male_attraction = -10;
    let declined = state.advance_validated(&view, 9, 1).unwrap().families;
    assert_eq!(declined.members[0].partner, None);
}
#[test]
fn genetic_desire_and_personal_circumstances_affect_choices() {
    let mut state = family();
    let view = View {
        reverse_affinity: 10,
        hunger: 0,
    };
    let preference = state.members[0].mate_preference.as_mut().unwrap();
    preference.desire = 500;
    preference.genetic_weight = 500;
    state.members[0].genome.mate_desire = [0; 2];
    assert!(
        state
            .advance_validated(&view, 9, 1)
            .unwrap()
            .families
            .members[0]
            .pregnancy
            .is_none()
    );
    state.members[0].genome.mate_desire = [1000; 2];
    assert_eq!(
        state
            .advance_validated(&view, 9, 1)
            .unwrap()
            .families
            .members[0]
            .pregnancy
            .as_ref()
            .unwrap()
            .father,
        3
    );
    state.members[0]
        .mate_preference
        .as_mut()
        .unwrap()
        .hunger_weight = -100;
    assert!(
        state
            .advance_validated(
                &View {
                    reverse_affinity: 10,
                    hunger: 10
                },
                9,
                1
            )
            .unwrap()
            .families
            .members[0]
            .pregnancy
            .is_none()
    );
}

#[test]
fn a_person_can_evaluate_existing_children_in_either_direction() {
    let mut state = family();
    let mut child = member(4, Sex::Female, None);
    child.age = 3;
    child.parents = vec![1, 3].into();
    state.members.push(child);
    state.next_id = 5;
    let view = View {
        reverse_affinity: 10,
        hunger: 0,
    };
    state.members[0]
        .mate_preference
        .as_mut()
        .unwrap()
        .dependent_weight = -1000;
    assert!(
        state
            .advance_validated(&view, 9, 1)
            .unwrap()
            .families
            .members[0]
            .pregnancy
            .is_none()
    );
    state.members[0]
        .mate_preference
        .as_mut()
        .unwrap()
        .dependent_weight = 100;
    assert_eq!(
        state
            .advance_validated(&view, 9, 1)
            .unwrap()
            .families
            .members[0]
            .pregnancy
            .as_ref()
            .unwrap()
            .father,
        3
    );
}
#[test]
fn results_do_not_depend_on_worker_count() {
    let state = family();
    let view = View {
        reverse_affinity: 10,
        hunger: 0,
    };
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| state.advance_validated(&view, 9, 1).unwrap().families)
    };
    assert_eq!(run(1), run(4));
}
#[test]
fn desire_alleles_are_inherited_from_both_parents() {
    let mut state = family();
    state.members[0].genome.mate_desire = [200, 200];
    state.members[2].genome.mate_desire = [800, 800];
    let view = View {
        reverse_affinity: 10,
        hunger: 0,
    };
    let conceived = state.advance_validated(&view, 9, 1).unwrap().families;
    let born = conceived.advance_validated(&view, 9, 2).unwrap().families;
    assert_eq!(born.members[3].genome.mate_desire, [200, 800]);
}

#[test]
#[ignore = "manual concentrated reproductive-choice scaling measurement"]
fn concentrated_mate_choice_benchmark() {
    let mut state = family();
    state.members = (1..=20_000)
        .map(|id| {
            let mut person = member(id, if id % 2 == 0 { Sex::Male } else { Sex::Female }, None);
            person.mate_preference = Some(MatePreference {
                desire: 1000,
                genetic_weight: 0,
                affinity_weight: 0,
                trust_weight: 0,
                fear_weight: 0,
                obligation_weight: 0,
                attraction_weight: 0,
                female_attraction: 0,
                male_attraction: 0,
                hunger_weight: 0,
                dependent_weight: 0,
                minimum: 0,
            });
            person
        })
        .collect();
    state.next_id = 20_001;
    let view = View {
        reverse_affinity: 0,
        hunger: 0,
    };
    for feelings in [false, true] {
        for person in &mut state.members {
            let preference = person.mate_preference.as_mut().unwrap();
            preference.trust_weight = i32::from(feelings);
            preference.fear_weight = i32::from(feelings);
            preference.obligation_weight = i32::from(feelings);
        }
        let mut reference = None;
        for workers in [1, 6] {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap();
            let mut durations = Vec::new();
            for _ in 0..4 {
                let started = std::time::Instant::now();
                let output =
                    pool.install(|| state.advance_validated(&view, 9, 1).unwrap().families);
                let duration = started.elapsed();
                if let Some(expected) = &reference {
                    assert_eq!(expected, &output);
                } else {
                    reference = Some(output);
                }
                durations.push(duration);
            }
            durations.remove(0);
            durations.sort();
            println!(
                "feelings={feelings}, {workers} workers: median {:?}",
                durations[1]
            );
        }
    }
}
