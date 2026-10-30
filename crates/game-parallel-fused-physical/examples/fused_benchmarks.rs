//! Run a fused benchmark by name; remaining arguments belong to the fixture.
#[path = "support/fused_admission.rs"]
mod fused_admission;
#[path = "support/fused_care.rs"]
mod fused_care;
#[path = "support/fused_combat.rs"]
mod fused_combat;
#[path = "support/fused_compare.rs"]
mod fused_compare;
#[path = "support/fused_economy.rs"]
mod fused_economy;
#[path = "support/fused_families.rs"]
mod fused_families;
#[path = "support/fused_parties.rs"]
mod fused_parties;
#[path = "support/fused_tactical_moves.rs"]
mod fused_tactical_moves;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    match std::env::args().nth(1).as_deref().unwrap_or("") {
        "fused_admission" => fused_admission::run(),
        "fused_care" => fused_care::run(),
        "fused_combat" => fused_combat::run(),
        "fused_compare" => fused_compare::run(),
        "fused_economy" => fused_economy::run(),
        "fused_families" => fused_families::run(),
        "fused_parties" => fused_parties::run(),
        "fused_tactical_moves" => fused_tactical_moves::run(),
        _ => Err("choose a fused benchmark fixture".into()),
    }
}
