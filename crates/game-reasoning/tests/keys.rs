use game_reasoning::*;
use std::{collections::BTreeMap, fmt};
fn legacy(text: &str) -> u64 {
    text.bytes().fold(14695981039346656037, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(1099511628211)
    })
}
#[test]
fn streaming_claim_keys_match_the_existing_wire_keys_and_propagate_writer_failure() {
    for n in [0, 1, 30, i64::MIN, i64::MAX] {
        let claim = Claim {
            comparisons: 3,
            complexity: 2,
            confidence: 80,
            context: BTreeMap::from([("warmth".into(), n), ("storage:é".into(), -2)]),
            effect: n,
            factor: "temperature".into(),
            outcome: "duration".into(),
            subject: "food:🙂".into(),
        };
        let context = claim
            .context
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(",");
        let expected = format!(
            "theory:{}:{}:{}:{:x}",
            claim.subject,
            claim.factor,
            claim.outcome,
            legacy(&context)
        );
        assert_eq!(claim.key(), expected);
        let mut hash = game_core::Fingerprint::default();
        claim.write_key(&mut hash).unwrap();
        assert_eq!(hash.finish(), legacy(&expected));
        struct Refusing;
        impl fmt::Write for Refusing {
            fn write_str(&mut self, _: &str) -> fmt::Result {
                Err(fmt::Error)
            }
        }
        assert!(claim.write_key(&mut Refusing).is_err());
    }
}
