use game_core::*;
use game_sim::*;
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Cancel;
impl Request for Cancel {
    const KEY: &'static str = "test.cancel";
    type Response = ();
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Restart;
impl Request for Restart {
    const KEY: &'static str = "test.restart";
    type Response = (Action, Action);
}
fn builder() -> Builder {
    let mut b = Builder::new();
    b.plugin("test", 1).unwrap();
    b.request::<Cancel>(|_, _| Ok(())).unwrap();
    b.request::<Restart>(|tx, _| {
        let person = PersonId(7);
        tx.start_action(person, "think", Some(SimTime(10)), Cancel)?;
        let first = tx.get::<Actions>()?.0[&person].clone();
        tx.cancel_action(person)?;
        tx.start_action(person, "think", Some(SimTime(10)), Cancel)?;
        Ok((first, tx.get::<Actions>()?.0[&person].clone()))
    })
    .unwrap();
    b
}
#[test]
fn identical_restarted_actions_have_distinct_identities_even_within_one_transaction() {
    let mut h = builder().build(1, 1).unwrap();
    let (first, second) = h.dispatch(Restart).unwrap();
    assert_ne!(first.id, second.id);
    assert_eq!(first.owner, second.owner);
    assert_eq!(first.kind, second.kind);
    assert_eq!(first.until, second.until);
    assert_eq!(first.cancel, second.cancel);
    let saved = h.save().unwrap();
    assert_eq!(saved, builder().load(&saved, 4).unwrap().save().unwrap());
    let mut legacy: serde_json::Value = serde_json::from_str(&saved).unwrap();
    legacy["resources"]["world.actions"]["7"]
        .as_object_mut()
        .unwrap()
        .remove("id");
    assert!(builder().load(&legacy.to_string(), 1).is_ok());
    let mut invalid: serde_json::Value = serde_json::from_str(&saved).unwrap();
    invalid["next_id"] = serde_json::json!(second.id);
    assert!(builder().load(&invalid.to_string(), 1).is_err());
}
