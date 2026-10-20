mod support;
use game_core::*;
use support::*;
#[test]
fn care_is_many_to_many_revocable_and_never_transferred_by_birth_or_death() {
    let (mut h, area) = world();
    let (a, _) = person(&mut h, area);
    let (b, _) = person(&mut h, area);
    let (c, _) = person(&mut h, area);
    h.dispatch(game_households::Enable).unwrap();
    let children: Vec<_> = (0..2)
        .map(|_| {
            h.dispatch(game_lifecycle::Birth {
                mother: a,
                father: b,
                attendant: a,
                area,
            })
            .unwrap()
        })
        .collect();
    assert!(
        h.get::<game_households::Households>()
            .unwrap()
            .caregivers
            .is_empty()
    );
    for caregiver in [a, b, c] {
        for beneficiary in &children {
            h.dispatch(game_households::CommitCare {
                caregiver,
                beneficiary: *beneficiary,
                accept: true,
            })
            .unwrap();
        }
    }
    assert_eq!(
        h.get::<game_households::Households>()
            .unwrap()
            .carers_of(children[0])
            .count(),
        3
    );
    assert_eq!(
        h.get::<game_households::Households>()
            .unwrap()
            .beneficiaries_of(c)
            .count(),
        2
    );
    h.dispatch(game_households::CommitCare {
        caregiver: c,
        beneficiary: children[0],
        accept: false,
    })
    .unwrap();
    assert!(
        !h.get::<game_households::Households>()
            .unwrap()
            .committed(c, children[0])
    );
    assert!(
        h.get::<game_households::Households>()
            .unwrap()
            .committed(c, children[1])
    );
    h.dispatch(game_physiology::Damage {
        person: a,
        amount: 1000,
    })
    .unwrap();
    assert_eq!(
        h.get::<game_households::Households>()
            .unwrap()
            .carers_of(children[0])
            .collect::<Vec<_>>(),
        vec![b]
    );
    let saved = h.save().unwrap();
    assert_eq!(builder().load(&saved, 4).unwrap().save().unwrap(), saved);
}
#[test]
fn no_kinship_or_care_commitment_is_needed_to_feed_a_perceived_local_need() {
    let (mut h, area) = world();
    // Isolate a visible food cue rather than competing fatigue and hunger cues.
    h.dispatch(game_physiology::Configure(game_physiology::Rules {
        fatigue_per_tick: 0,
        ..Default::default()
    }))
    .unwrap();
    let (helper, pack) = person(&mut h, area);
    let (patient, _) = person(&mut h, area);
    h.dispatch(game_households::Enable).unwrap();
    h.dispatch(game_communication::Enable).unwrap();
    h.dispatch(game_traits::Set {
        person: helper,
        profile: [
            ("intelligence".into(), 4000),
            ("interpretation".into(), 1000),
            ("altruism".into(), 1000),
            ("empathy".into(), 1000),
        ]
        .into(),
    })
    .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory: pack,
        good: game_cargo::FOOD,
        quantity: 100,
    })
    .unwrap();
    h.dispatch(game_physiology::Start).unwrap();
    let until = h.now().after(SimDuration(35 * MINUTE.0)).unwrap();
    while h.advance(until, 128).unwrap().stop == game_sim::Stop::Budget {}
    h.dispatch(game_physiology::Eat {
        person: helper,
        inventory: pack,
    })
    .unwrap();
    let before = h.get::<game_physiology::Bodies>().unwrap().people[&patient].hunger;
    h.dispatch(game_care::Assist {
        caregiver: helper,
        beneficiary: patient,
    })
    .unwrap();
    assert!(h.get::<game_physiology::Bodies>().unwrap().people[&patient].hunger < before);
    assert!(
        h.get::<game_households::Households>()
            .unwrap()
            .caregivers
            .is_empty()
    );
}

#[test]
fn declaring_self_reliance_does_not_rewrite_another_persons_care_intentions() {
    let (mut h, area) = world();
    let (helper, _) = person(&mut h, area);
    let (beneficiary, _) = person(&mut h, area);
    h.dispatch(game_households::CommitCare {
        caregiver: helper,
        beneficiary,
        accept: true,
    })
    .unwrap();
    h.dispatch(game_households::Independent {
        person: beneficiary,
    })
    .unwrap();
    let households = h.get::<game_households::Households>().unwrap();
    assert!(households.self_reliant.contains_key(&beneficiary));
    assert!(households.committed(helper, beneficiary));
    h.dispatch(game_households::CommitCare {
        caregiver: helper,
        beneficiary,
        accept: false,
    })
    .unwrap();
    assert!(
        !h.get::<game_households::Households>()
            .unwrap()
            .committed(helper, beneficiary)
    );
}
