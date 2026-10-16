mod support;
use game_core::*;
use game_knowledge::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
use support::*;
#[derive(Clone, Default, Serialize, Deserialize)]
struct Audit {
    count: u64,
    deny: bool,
}
impl Resource for Audit {
    const KEY: &'static str = "scene.audit";
}
#[derive(Clone, Serialize, Deserialize)]
struct Deny;
impl Request for Deny {
    const KEY: &'static str = "scene.deny";
    type Response = ();
}
#[test]
fn batch_observation_keeps_per_patch_hooks_and_their_atomic_failures() {
    let mut b = builder();
    game_ecology::plugin(&mut b).unwrap();
    b.plugin("audit", 1).unwrap();
    b.resource::<Audit>().unwrap();
    b.request::<Deny>(|tx, _| {
        tx.edit::<Audit>()?.deny = true;
        Ok(())
    })
    .unwrap();
    b.before::<ObservePatch>("audit.patch", |tx, _| {
        tx.edit::<Audit>()?.count += 1;
        require(!tx.get::<Audit>()?.deny, "observation denied")
    })
    .unwrap();
    let mut host = b.build(1, 1).unwrap();
    host.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    let area = host
        .dispatch(game_land::CreateArea {
            name: "home".into(),
        })
        .unwrap();
    let person = host
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: SimTime(0),
        })
        .unwrap();
    host.dispatch(game_transport::Place { person, area })
        .unwrap();
    let patch = host
        .dispatch(game_ecology::Create(game_ecology::Patch {
            area,
            capacity: 10,
            stock: 10,
            renew: 0,
            good: game_cargo::FOOD,
        }))
        .unwrap();
    host.dispatch(ObserveSurroundings { person }).unwrap();
    assert_eq!(host.get::<Audit>().unwrap().count, 1);
    assert!(view(&host, person).unwrap().contains_key(&patch_key(patch)));
    host.dispatch(Deny).unwrap();
    let before = host.save().unwrap();
    assert!(host.dispatch(ObserveSurroundings { person }).is_err());
    assert_eq!(host.save().unwrap(), before);
}
#[test]
fn scenario_facts_are_validated_and_cannot_overwrite_existing_knowledge() {
    let mut f = setup();
    let facts = Facts::from_iter([(
        "history:one".into(),
        Fact {
            value: std::sync::Arc::new(serde_json::json!("story").into()),
            source: f.ruler,
            observed: SimTime(0),
            revision: 0,
            confidence: 90,
        },
    )]);
    f.host
        .dispatch(Initialize {
            person: f.other,
            facts: facts.clone(),
        })
        .unwrap();
    let before = f.host.save().unwrap();
    assert!(
        f.host
            .dispatch(Initialize {
                person: f.other,
                facts: Facts::default()
            })
            .is_err()
    );
    assert_eq!(f.host.save().unwrap(), before);
    f.host
        .dispatch(ObserveSurroundings { person: f.other })
        .unwrap();
    assert!(view(&f.host, f.other).unwrap().contains_key("history:one"));
    let mut invalid = facts;
    invalid.insert(
        "bad".into(),
        Fact {
            value: std::sync::Arc::new(serde_json::json!(0).into()),
            source: f.ruler,
            observed: SimTime(0),
            revision: 0,
            confidence: 101,
        },
    );
    let before = f.host.save().unwrap();
    assert!(
        f.host
            .dispatch(Initialize {
                person: f.guardian,
                facts: invalid
            })
            .is_err()
    );
    assert_eq!(f.host.save().unwrap(), before);
}
