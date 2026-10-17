#![allow(dead_code)]
use game_core::*;
use game_sim::*;
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Idle(pub PersonId);
impl Request for Idle {
    const KEY: &'static str = "test.idle";
    type Response = ();
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Cancel(pub PersonId);
impl Request for Cancel {
    const KEY: &'static str = "test.cancel";
    type Response = ();
}
pub fn builder() -> Builder {
    let mut b = Builder::new();
    game_land::plugin(&mut b).unwrap();
    game_transport::plugin(&mut b).unwrap();
    game_objects::plugin(&mut b).unwrap();
    game_cargo::plugin(&mut b).unwrap();
    game_property::plugin(&mut b).unwrap();
    game_physiology::plugin(&mut b).unwrap();
    game_skills::plugin(&mut b).unwrap();
    game_relationships::plugin(&mut b).unwrap();
    game_traits::plugin(&mut b).unwrap();
    game_lineage::plugin(&mut b).unwrap();
    game_environment::plugin(&mut b).unwrap();
    game_agriculture::plugin(&mut b).unwrap();
    game_horticulture::plugin(&mut b).unwrap();
    b.plugin("test", 1).unwrap();
    b.request::<Idle>(|tx, r| tx.notify(game_characters::IdleReview { person: r.0 }))
        .unwrap();
    b.request::<Cancel>(|tx, r| tx.cancel_action(r.0)).unwrap();
    b
}
pub fn setup() -> (Host, PersonId, InventoryId, AreaId) {
    let mut h = builder().build(17, 1).unwrap();
    h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    h.dispatch(game_traits::Install(
        game_traits::load(&mut game_content::Store::standard()).unwrap(),
    ))
    .unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "Grove".into(),
        })
        .unwrap();
    let person = person(&mut h, area);
    let pack = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(person),
            capacity: 2000,
        })
        .unwrap();
    h.dispatch(game_property::ClaimInventory {
        claimant: person,
        inventory: pack,
        recognized_by: [person].into(),
    })
    .unwrap();
    (h, person, pack, area)
}
pub fn person(h: &mut Host, area: AreaId) -> PersonId {
    let person = h
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: h.now(),
        })
        .unwrap();
    h.dispatch(game_transport::Place { person, area }).unwrap();
    person
}
pub fn good(h: &Host, k: &str) -> GoodId {
    h.get::<game_objects::Objects>()
        .unwrap()
        .resolve(k)
        .unwrap()
}
pub fn deposit(h: &mut Host, i: InventoryId, k: &str, n: u64) {
    let good = good(h, k);
    h.dispatch(game_cargo::Deposit {
        inventory: i,
        good,
        quantity: n,
    })
    .unwrap();
}
pub fn amount(h: &Host, i: InventoryId, k: &str) -> u64 {
    h.get::<game_cargo::Cargo>().unwrap().0[&i].amount(good(h, k))
}
pub fn run(h: &mut Host, at: u64) {
    while h.advance(SimTime(at), 128).unwrap().stop == Stop::Budget {}
}
