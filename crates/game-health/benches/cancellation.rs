//! Actual Health cancellation preparation, final guards, and publication.
use game_core::*;
use game_ecs::{Batch, Schema, StableId, Store};
use game_health::*;
use std::{collections::BTreeMap, time::Instant};

fn fixture(population: u64) -> (Runtime, Store, Vec<PersonId>) {
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    let mut store = Store::with_schema(schema);
    let mut batch = Batch::new();
    let actors = (1..=population).map(PersonId).collect::<Vec<_>>();
    for actor in &actors {
        let patient = PersonId(actor.0 % population + 1);
        batch.insert(
            StableId(actor.0),
            Job {
                actor: *actor,
                area: AreaId(1),
                completes: SimTime(1000),
                work: Work::Treat {
                    patient,
                    skill_ms: 0,
                    elapsed: 0,
                },
            },
        );
        batch.insert(
            StableId(actor.0),
            Offers {
                caregiver: *actor,
                patients: BTreeMap::from([(
                    patient,
                    Offer {
                        caregiver: *actor,
                        patient,
                        herbs: InventoryId(population + 1),
                        water: InventoryId(population + 2),
                        expires: SimTime(1000),
                        accepted: false,
                    },
                )]),
            },
        );
    }
    store.apply_batch(batch).unwrap();
    (Runtime::default(), store, actors)
}
fn main() {
    let samples = std::env::args()
        .skip(1)
        .find(|argument| argument != "--bench")
        .map(|value| value.parse::<usize>().expect("sample count"))
        .unwrap_or(3);
    assert!(samples > 0);
    println!(
        "population,sample,prepare_ms,opening_validation_ms,stage_ms,final_guard_ms,apply_ms,total_ms"
    );
    for population in [1000, 2000, 4000] {
        let (runtime, opening, actors) = fixture(population);
        for sample in 0..samples {
            let mut store = opening.clone();
            let mut batch = Batch::new();
            let started = Instant::now();
            let prepared = runtime.prepare_cancel_many(&store, &actors);
            let prepared_at = Instant::now();
            prepared.validate(&runtime, &store).unwrap();
            let validated_at = Instant::now();
            let finish = prepared.stage_into(&runtime, &store, &mut batch).unwrap();
            let staged_at = Instant::now();
            finish.validate_batch(&runtime, &store, &batch).unwrap();
            let guarded_at = Instant::now();
            store.apply_batch(batch).unwrap();
            let applied_at = Instant::now();
            let groups = finish.finish();
            assert_eq!(groups.len(), population as usize);
            assert_eq!(
                groups
                    .iter()
                    .flat_map(|(_, effects)| effects)
                    .filter(|effect| matches!(effect, HealthEffect::ClearWork { .. }))
                    .count(),
                population as usize
            );
            assert_eq!(jobs(&store).count(), 0);
            assert_eq!(offer_rows(&store).count(), 0);
            let milliseconds = |from: Instant, to: Instant| (to - from).as_secs_f64() * 1000.0;
            println!(
                "{population},{sample},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3}",
                milliseconds(started, prepared_at),
                milliseconds(prepared_at, validated_at),
                milliseconds(validated_at, staged_at),
                milliseconds(staged_at, guarded_at),
                milliseconds(guarded_at, applied_at),
                milliseconds(started, applied_at)
            );
        }
    }
}
