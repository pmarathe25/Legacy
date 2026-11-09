use game_economy_claims::{Claims, FieldClaim, FieldUse, Preference, publish_field_uses};
use game_economy_person::Person;

fn person(id: u64) -> Person {
    Person {
        id,
        region: Some(1),
        health: 100,
        hunger: 0,
        hunger_remainder: 0,
        immortal: None,
        willing_to_work: false,
        seek_meals: false,
        controlled: false,
    }
}

#[test]
fn scoped_requests_and_individual_disregard_do_not_erase_competing_assertions() {
    let mut claims = Claims {
        field_claims: vec![
            FieldClaim {
                person: 1,
                field: 9,
            },
            FieldClaim {
                person: 3,
                field: 9,
            },
        ],
        preferences: vec![Preference {
            person: 2,
            disregard_others: false,
            assert_outputs: false,
            assert_built_sites: false,
            assert_cultivated_fields: false,
            desperate_hunger: None,
        }],
        ..Default::default()
    };
    assert!(!claims.index().allows_field(2, 9, Some(1)));
    claims.field_claims.pop();
    assert!(claims.index().allows_field(2, 9, Some(1)));
    assert!(!claims.index().allows_field(2, 9, None));
    claims.field_claims.push(FieldClaim {
        person: 3,
        field: 9,
    });
    claims.preferences[0].disregard_others = true;
    assert!(claims.index().allows_field(2, 9, None));
    assert_eq!(claims.field_claims.len(), 2);
}

#[test]
fn field_observations_require_local_presence_and_contact() {
    let mut claims = Claims {
        field_claims: vec![
            FieldClaim {
                person: 1,
                field: 9,
            },
            FieldClaim {
                person: 3,
                field: 9,
            },
            FieldClaim {
                person: 4,
                field: 9,
            },
        ],
        ..Default::default()
    };
    let people = vec![person(1), person(2), person(3), person(4)];
    let present = |id, _region| id != 3;
    let contact = |id, _actor| id != 4;
    publish_field_uses(
        &mut claims,
        &[FieldUse {
            person: 2,
            field: 9,
            harvest: false,
        }],
        &[(9, 1)],
        &people,
        1,
        Some(&present),
        Some(&contact),
    );
    assert_eq!(claims.field_events.len(), 1);
    assert_eq!(claims.field_events[0].claimant, 1);
}
