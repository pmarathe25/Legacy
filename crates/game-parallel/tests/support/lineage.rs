use game_parallel::{LineageEventKind, World};
fn world() -> World {
    World::from_toml(include_str!("../../../../content/parallel/lineage.toml")).unwrap()
}
#[test]
fn succession_waits_for_an_actual_birth_after_death_and_survives_reload() {
    let mut w = world();
    let ruler = w
        .families
        .as_mut()
        .unwrap()
        .members
        .iter_mut()
        .find(|m| m.id == 1)
        .unwrap();
    ruler.age = 69;
    w.step().unwrap();
    let lineage = w.lineage.as_ref().unwrap();
    assert_eq!(lineage.current, None);
    assert_eq!(lineage.waiting_since, Some(1));
    assert_eq!(lineage.events[0].kind, LineageEventKind::Death);
    w = World::from_json(&serde_json::to_string(&w).unwrap()).unwrap();
    for _ in 0..8 {
        w.step().unwrap();
        if w.lineage.as_ref().unwrap().current.is_some() {
            break;
        }
    }
    let lineage = w.lineage.as_ref().unwrap();
    let heir = lineage.current.unwrap();
    let member = w
        .families
        .as_ref()
        .unwrap()
        .members
        .iter()
        .find(|m| m.id == heir)
        .unwrap();
    assert!(member.born.is_some_and(|tick| tick > 1));
    assert_eq!(lineage.history[0].person, 1);
    assert_eq!(lineage.history[0].ended, 1);
    w.validate().unwrap();
}
#[test]
fn no_birth_leaves_an_indefinite_ruler_gap() {
    let mut w = world();
    for member in &mut w.families.as_mut().unwrap().members {
        member.reproduction_allowed = false;
    }
    w.families
        .as_mut()
        .unwrap()
        .members
        .iter_mut()
        .find(|m| m.id == 1)
        .unwrap()
        .age = 69;
    for _ in 0..20 {
        w.step().unwrap();
    }
    let lineage = w.lineage.as_ref().unwrap();
    assert!(lineage.current.is_none());
    assert_eq!(lineage.history.len(), 1);
    assert_eq!(w.families.as_ref().unwrap().totals.births, 0);
}
#[test]
fn lineage_requires_family_state_and_rejects_invalid_current_identity() {
    let mut w = world();
    w.families = None;
    assert!(w.validate().is_err());
    let before = w.clone();
    assert!(w.step().is_err());
    assert_eq!(w, before);
    let mut w = world();
    w.lineage.as_mut().unwrap().current = Some(999);
    assert!(w.validate().is_err());
    let before = w.clone();
    assert!(w.step().is_err());
    assert_eq!(w, before);
}
