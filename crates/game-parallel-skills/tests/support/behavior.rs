use crate::{Attribution, Completion, Rule, Skills};
use std::collections::HashSet;

fn skills() -> Skills {
    Skills {
        rules: vec![Rule {
            recipe: 7,
            completions_per_level: 2,
            max_level: 3,
            work_per_level: 2,
        }],
        records: vec![],
        aliases: vec![],
        catalog_training: None,
        events: vec![],
        lessons: vec![],
        teachers: vec![],
        inherit_teachers: false,
    }
}

fn credit(job: u64, worker: u64) -> Completion {
    Completion {
        job,
        worker,
        recipe: 7,
        finished_at: 1,
    }
}

fn validate(state: &Skills, now: u64) {
    state
        .validate(&HashSet::from([11, 99]), &HashSet::from([7, 8]), now)
        .unwrap();
}

#[test]
fn learning_is_personal_and_successive_levels_require_more_work() {
    let initial = skills();
    let initial_index = initial.index();
    let first = initial.prepare(&[credit(1, 11), credit(2, 11)], 1);
    assert_eq!(initial_index.rate(11, 7), 1);
    assert!(initial.records.is_empty(), "preparation must not publish");
    assert_eq!(first.index().rate(11, 7), 3);
    assert_eq!(first.index().rate(99, 7), 1);
    assert_eq!(first.index().rate(11, 8), 1);
    let second = first.prepare(&[credit(3, 11), credit(4, 11)], 2);
    assert_eq!(second.index().rate(11, 7), 3);
    assert!(second.events.is_empty());
    let third = second.prepare(&[credit(5, 11), credit(6, 11)], 3);
    assert_eq!(third.index().rate(11, 7), 5);
    assert_eq!(third.events[0].previous_level, 1);
    assert_eq!(third.events[0].level, 2);
    validate(&third, 3);
}

#[test]
fn related_recipe_experience_and_teaching_share_one_personal_skill() {
    let mut initial = skills();
    initial.aliases.push(crate::Alias {
        recipe: 8,
        canonical: 7,
    });
    validate(&initial, 0);
    let mut first = credit(1, 11);
    first.recipe = 8;
    let trained = initial.prepare(&[first, credit(2, 11)], 1);
    assert_eq!(trained.index().rate(11, 7), 3);
    assert_eq!(trained.index().rate(11, 8), 3);
    assert_eq!(trained.index().rate(99, 8), 1);
    assert_eq!(trained.records.len(), 1);
    assert_eq!(trained.records[0].recipe, 7);
    assert_eq!(trained.prepare(&[first], 1).records, trained.records);
    let mut taught = trained;
    taught.teach(11, 99, 8, 2, 3, None).unwrap();
    taught.teach(11, 99, 8, 3, 3, None).unwrap();
    assert_eq!(taught.index().rate(99, 7), 3);
    assert_eq!(taught.index().rate(99, 8), 3);
    assert!(taught.lessons.iter().all(|lesson| lesson.recipe == 7));
    validate(&taught, 3);
    let restored: Skills = serde_json::from_str(&serde_json::to_string(&taught).unwrap()).unwrap();
    assert_eq!(restored, taught);
}

#[test]
fn duplicate_and_out_of_order_finished_receipts_are_idempotent_after_reload() {
    let first = skills().prepare(&[credit(9, 11), credit(4, 11), credit(9, 11)], 1);
    assert_eq!(first.records[0].completions, 2);
    let saved = serde_json::to_string(&first).unwrap();
    let restored: Skills = serde_json::from_str(&saved).unwrap();
    let repeated = restored.prepare(&[credit(4, 11), credit(9, 11)], 2);
    assert_eq!(repeated.records, first.records);
    assert!(repeated.events.is_empty());
    let next = repeated.prepare(&[credit(2, 11), credit(15, 11)], 3);
    assert_eq!(
        next.records[0].credited_jobs,
        [2, 4, 9, 15].map(|job| Attribution {
            job,
            finished_at: 1
        })
    );
    validate(&next, 3);
}

#[test]
fn waiting_for_storage_and_discarded_drafts_do_not_train_again() {
    let initial = skills();
    let credits = [credit(1, 11), credit(2, 11)];
    let discarded = initial.prepare(&credits, 1);
    let retried = initial.prepare(&credits, 1);
    assert_eq!(discarded, retried);
    assert!(initial.records.is_empty());
    let waiting = retried.prepare(&[], 2);
    assert_eq!(waiting.records, retried.records);
    assert!(waiting.events.is_empty());
}

#[test]
fn mastery_caps_rates_and_discards_attribution_storage() {
    let credits: Vec<_> = (0..100).map(|job| credit(job, 11)).collect();
    let mastered = skills().prepare(&credits, 1);
    assert_eq!(mastered.records[0].completions, 12);
    assert!(mastered.records[0].credited_jobs.is_empty());
    assert_eq!(mastered.index().rate(11, 7), 7);
    let repeated = mastered.prepare(&credits, 2);
    assert_eq!(repeated.records, mastered.records);
    assert!(repeated.events.is_empty());
    validate(&repeated, 2);
}

#[test]
fn unconfigured_recipes_do_not_create_skill_state() {
    let state = skills().prepare(
        &[Completion {
            job: 1,
            worker: 11,
            recipe: 8,
            finished_at: 1,
        }],
        1,
    );
    assert!(state.records.is_empty());
    assert!(state.events.is_empty());
}

#[test]
fn worker_count_and_credit_order_do_not_change_learning() {
    let initial = skills();
    let credits: Vec<_> = (0..20_000)
        .rev()
        .map(|job| credit(job, if job % 2 == 0 { 11 } else { 99 }))
        .collect();
    let serial = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap()
        .install(|| initial.prepare(&credits, 1));
    let mut reversed = credits.clone();
    reversed.reverse();
    let parallel = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap()
        .install(|| initial.prepare(&reversed, 1));
    assert_eq!(serial, parallel);
    validate(&parallel, 1);
}

#[test]
fn malformed_attribution_and_unknown_people_are_rejected() {
    let mut state = skills().prepare(&[credit(1, 11), credit(2, 11)], 1);
    state.records[0].credited_jobs[1] = state.records[0].credited_jobs[0];
    assert!(
        state
            .validate(&HashSet::from([11]), &HashSet::from([7]), 1)
            .is_err()
    );
    let state = skills().prepare(&[credit(1, 99)], 1);
    assert!(
        state
            .validate(&HashSet::from([11]), &HashSet::from([7]), 1)
            .is_err()
    );
}

#[test]
fn largest_rule_values_have_checked_numeric_bounds() {
    let mut state = skills();
    state.rules[0].completions_per_level = u32::MAX;
    state.rules[0].work_per_level = u32::MAX;
    state.rules[0].max_level = u16::MAX;
    validate(&state, 0);
    let cap = u64::from(u32::MAX) * u64::from(u16::MAX) * (u64::from(u16::MAX) + 1) / 2;
    state.records.push(crate::Record {
        person: 11,
        recipe: 7,
        completions: cap,
        credited_jobs: vec![],
    });
    validate(&state, 0);
    assert_eq!(
        state.index().rate(11, 7),
        1 + u64::from(u16::MAX) * u64::from(u32::MAX)
    );
}

#[test]
fn lessons_and_work_together_build_personal_skill_and_validate_after_reload() {
    let mut state = skills();
    state.teach(99, 11, 7, 1, 7, None).unwrap();
    assert_eq!(state.index().rate(11, 7), 1);
    assert!(state.teach(99, 11, 7, 1, 7, None).is_err());
    let saved = serde_json::to_string(&state).unwrap();
    let state: Skills = serde_json::from_str(&saved).unwrap();
    validate(&state, 1);
    let state = state.prepare(&[credit(4, 11)], 1);
    assert_eq!(state.index().rate(11, 7), 3);
    assert_eq!(state.records[0].completions, 1);
    assert_eq!(state.lessons.len(), 1);
    assert_eq!(
        (state.events[0].previous_level, state.events[0].level),
        (0, 1)
    );
    validate(&state, 1);
}

#[test]
fn independent_lesson_index_is_worker_invariant() {
    let mut state = skills();
    state.lessons = (1..=10_000)
        .map(|student| crate::Lesson {
            teacher: 99_999,
            student,
            recipe: 7,
            tick: 1,
            source_life: None,
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let index = state.index();
                (1..=10_000)
                    .map(|student| index.rate(student, 7))
                    .collect::<Vec<_>>()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn independent_teachers_transfer_lessons_in_parallel_without_duplicate_credits() {
    let mut state = skills();
    state.records = (1..=5_000)
        .map(|person| crate::Record {
            person,
            recipe: 7,
            completions: 12,
            credited_jobs: vec![],
        })
        .collect();
    state.teachers = (1..=5_000)
        .map(|person| crate::TeacherPolicy {
            person,
            recipe: 7,
            enabled: true,
            eligible_at: 0,
        })
        .collect();
    let pairs: Vec<_> = (1..=5_000)
        .map(|person| (person, person + 10_000))
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut next = state.clone();
                let draft = state.plan_teaching(&pairs, None, 1).unwrap();
                assert!(state.lessons.is_empty());
                draft.commit(&mut next);
                assert!(next.plan_teaching(&pairs, None, 1).is_none());
                next
            })
    };
    let one = run(1);
    assert_eq!(one, run(4));
    assert_eq!(one.lessons.len(), 5_000);
    assert!(
        one.lessons
            .iter()
            .all(|lesson| lesson.source_life.is_none())
    );
}

#[test]
fn inherited_teaching_roles_begin_only_at_adulthood() {
    let mut state = skills();
    state.records.push(crate::Record {
        person: 11,
        recipe: 7,
        completions: 12,
        credited_jobs: vec![],
    });
    state.teachers.push(crate::TeacherPolicy {
        person: 11,
        recipe: 7,
        enabled: true,
        eligible_at: 10,
    });
    assert!(state.plan_teaching(&[(11, 99)], None, 9).is_none());
    assert!(state.plan_teaching(&[(11, 99)], None, 10).is_some());
}
