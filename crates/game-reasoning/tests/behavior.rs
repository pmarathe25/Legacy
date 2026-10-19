use game_reasoning::*;
use std::collections::BTreeMap;
fn observation(heat: i64, water: i64, value: i64, trial: u64) -> Observation {
    Observation {
        subject: "iron".into(),
        context: BTreeMap::from([("heat".into(), heat), ("water".into(), water)]),
        outcome: "hardness".into(),
        value,
        trial,
    }
}
#[test]
fn contextual_discovery_needs_attention_and_preserves_its_conditions() {
    let samples = [observation(10, 1, 5, 1), observation(20, 1, 15, 1)];
    assert!(infer(&samples, 100, 0).is_empty());
    assert!(infer(&samples, 0, 3).is_empty());
    let claims = infer(&samples, 100, 3);
    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0].factor, "heat");
    assert_eq!(claims[0].effect, 100);
    assert!(!claims[0].predicts(&BTreeMap::from([("water".into(), 0)])));
    assert!(claims[0].predicts(&BTreeMap::from([("water".into(), 1)])));
}
#[test]
fn uncontrolled_associations_stay_tentative_and_repeated_one_trial_is_not_independent_replication()
{
    let mut samples = vec![];
    for _ in 0..10 {
        samples.extend([observation(10, 1, 5, 0), observation(20, 1, 15, 0)]);
    }
    assert!(infer(&samples, 1000, 3).iter().all(|c| c.confidence <= 30));
    for sample in &mut samples {
        sample.trial = 1;
    }
    assert!(infer(&samples, 1000, 3).iter().all(|c| c.confidence <= 40));
}
#[test]
fn deductions_require_capacity_keep_assumptions_and_reduce_uncertainty() {
    let first = infer(
        &[observation(10, 1, 5, 1), observation(20, 1, 15, 1)],
        100,
        3,
    )
    .remove(0);
    let mut second = first.clone();
    second.factor = "hardness".into();
    second.outcome = "durability".into();
    second.effect = 200;
    second.confidence = 10;
    assert!(deduce(&[first.clone(), second.clone()], 100, 3).is_empty());
    let derived = deduce(&[first, second], 100, 8).remove(0);
    assert_eq!(derived.effect, 200);
    assert_eq!(derived.confidence, 7);
    assert_eq!(derived.context["water"], 1);
}
#[test]
fn independent_subjects_cannot_be_chained_as_if_they_were_one_material() {
    let first = infer(
        &[observation(10, 1, 5, 1), observation(20, 1, 15, 1)],
        100,
        3,
    )
    .remove(0);
    let mut second = first.clone();
    second.subject = "wood".into();
    second.factor = "hardness".into();
    assert!(deduce(&[first, second], 100, 8).is_empty());
}

#[test]
fn forecasts_respect_conditions_and_do_not_count_duplicate_mechanisms_twice() {
    let baseline = observation(20, 1, 15, 0);
    let first = infer(&[observation(10, 1, 5, 1), baseline.clone()], 100, 3).remove(0);
    let mut duplicate = first.clone();
    duplicate.complexity += 1;
    duplicate.confidence = 5;
    duplicate.effect = 1000000;
    let prediction = predict(
        &[first.clone(), duplicate],
        &baseline,
        &[("heat".into(), 30)].into(),
    )
    .unwrap();
    assert_eq!(
        prediction,
        predict(
            std::slice::from_ref(&first),
            &baseline,
            &[("heat".into(), 30)].into()
        )
        .unwrap()
    );
    assert!(
        predict(
            &[first],
            &baseline,
            &[("heat".into(), 30), ("water".into(), 0)].into()
        )
        .is_none()
    );
}

#[test]
fn abstraction_requires_agreement_across_conditions_and_retains_untested_assumptions() {
    let a = infer(
        &[observation(10, 1, 5, 1), observation(20, 1, 15, 1)],
        100,
        3,
    )
    .remove(0);
    let mut b = a.clone();
    b.context.insert("water".into(), 2);
    let learned = deduce(&[a.clone(), b.clone()], 100, 10);
    let general = learned.iter().find(|c| c.context.is_empty()).unwrap();
    assert!(general.complexity > a.complexity);
    assert!(general.confidence < a.confidence);
    assert!(deduce(&[a.clone(), b.clone()], 100, 4).is_empty());
    b.effect = -b.effect;
    assert!(deduce(&[a, b], 100, 10).is_empty());
}
