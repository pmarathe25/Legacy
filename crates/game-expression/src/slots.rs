//! Stable slots for developmental attributes; other authored traits use the ordinary path.
pub fn ability_slot(key: &str) -> Option<usize> {
    Some(match key {
        "intelligence" => 0,
        "attention" => 1,
        "learning" => 2,
        "foresight" => 3,
        "resilience" => 4,
        "interpretation" => 5,
        "hearing" => 6,
        "strength" => 7,
        "speed" => 8,
        "endurance" => 9,
        "agility" => 10,
        "hardiness" => 11,
        "robustness" => 12,
        "size" => 13,
        "constitution" => 14,
        "metabolism" => 15,
        "recovery" => 16,
        "vocalization" => 17,
        "gesturing" => 18,
        _ => return None,
    })
}
pub fn disposition_slot(key: &str) -> Option<usize> {
    Some(match key {
        "patience" => 19,
        "risk-tolerance" => 20,
        "sociability" => 21,
        "empathy" => 22,
        "compassion" => 23,
        "rigidity" => 24,
        _ => return None,
    })
}
