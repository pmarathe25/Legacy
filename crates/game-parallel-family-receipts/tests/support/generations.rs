use crate::{Frame, Kind, Row};
use game_parallel_families::FamilyEvent;

fn fixture() -> Vec<FamilyEvent> {
    (0..12000)
        .rev()
        .map(|i| {
            let kind = [
                Kind::Adulthood,
                Kind::Birth,
                Kind::Conception,
                Kind::Death,
                Kind::Partnership,
                Kind::PregnancyLoss,
            ][i % 6];
            FamilyEvent {
                tick: 7,
                kind: kind.name().into(),
                people: (0..kind.arity())
                    .map(|offset| 100 + i as u64 * 17 + offset as u64)
                    .collect(),
                region: (i % 3 != 0).then_some(99),
            }
        })
        .collect()
}
#[test]
fn compact_generation_matches_reference_sort_and_export_at_all_worker_counts() {
    let authored = fixture();
    let mut expected = authored.clone();
    expected.sort_unstable_by(|a, b| (&a.kind, &a.people).cmp(&(&b.kind, &b.people)));
    for workers in [1, 2, 4, 6] {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let initial = Frame::import(7, &authored).unwrap();
                assert_eq!(initial.export(), authored);
                let frame = Frame::canonical(7, initial.rows().to_vec());
                assert_eq!(frame.export(), expected);
                assert_eq!(frame.counts(), [2000; 6]);
                let births: Vec<_> = expected
                    .iter()
                    .filter(|e| e.kind == "birth")
                    .map(|e| e.people[0])
                    .collect();
                assert_eq!(frame.people(Kind::Birth).collect::<Vec<_>>(), births);
                assert_eq!(frame.tick(), 7);
                assert_eq!(initial.export(), authored);
            });
    }
}
#[test]
fn discarded_and_replaced_generations_leave_old_receipts_intact() {
    let events = fixture();
    let old = Frame::import(7, &events).unwrap();
    let receipt = old.clone();
    let discarded = Frame::canonical(8, vec![Row::new(Kind::Birth, [55, 1, 2], Some(10))]);
    drop(discarded);
    let current = Frame::canonical(8, vec![Row::new(Kind::Death, [55, 999, 999], None)]);
    drop(old);
    assert_eq!(receipt.export(), events);
    assert_eq!(current.rows()[0].people(), &[55]);
    assert_eq!(current.rows()[0].region(), None);
    assert_eq!(current.rows()[0].kind(), Kind::Death);
}
#[test]
fn malformed_import_is_rejected_without_changing_existing_generation() {
    let events = fixture();
    let old = Frame::import(7, &events).unwrap();
    let mut malformed = events.clone();
    malformed[0].kind = "unknown".into();
    assert!(Frame::import(7, &malformed).is_err());
    malformed[0] = events[0].clone();
    malformed[0].people.clear();
    assert!(Frame::import(7, &malformed).is_err());
    assert!(Frame::import(8, &events).is_err());
    assert_eq!(old.export(), events);
}
