use game_core::*;
use game_land::*;
use game_sim::*;
#[test]
fn invalid_roads_leave_ids_and_links_unchanged() {
    let mut b = Builder::new();
    plugin(&mut b).unwrap();
    let mut h = b.build(1, 1).unwrap();
    let a = h.dispatch(CreateArea { name: "a".into() }).unwrap();
    let before = h.save().unwrap();
    assert!(
        h.dispatch(Link {
            from: a,
            to: AreaId(99),
            duration: MINUTE
        })
        .is_err()
    );
    assert_eq!(before, h.save().unwrap());
    let b = h.dispatch(CreateArea { name: "b".into() }).unwrap();
    assert_eq!(b, AreaId(a.0 + 1));
    h.dispatch(Link {
        from: a,
        to: b,
        duration: MINUTE,
    })
    .unwrap();
    assert_eq!(h.get::<Land>().unwrap().0[&b].links[&a], MINUTE);
}
