use game_characters::*;
#[test]
fn urgent_survival_beats_ambition_and_commitment_avoids_oscillation() {
    let a = Candidate {
        id: 1,
        need: 20,
        goal: 0,
        value: 0,
        effort: 0,
        risk: 0,
        survival: true,
    };
    let b = Candidate {
        id: 2,
        need: 0,
        goal: 100,
        value: 0,
        effort: 0,
        risk: 0,
        survival: false,
    };
    assert_eq!(choose(&[a, b], None, 5, true).unwrap().id, 1);
    let mut better = a;
    better.id = 2;
    better.goal = 4;
    assert_eq!(choose(&[a, better], Some(1), 5, false).unwrap().id, 1);
}
