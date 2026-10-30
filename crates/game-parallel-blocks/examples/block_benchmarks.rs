//! Run a block-engine benchmark by name; remaining arguments belong to the fixture.
#[path = "support/autowork.rs"]
mod autowork;
#[path = "support/bandwidth.rs"]
mod bandwidth;
#[path = "support/block_throughput.rs"]
mod block_throughput;
#[path = "support/economy.rs"]
mod economy;
#[path = "support/families.rs"]
mod families;
#[path = "support/production.rs"]
mod production;
#[path = "support/travel.rs"]
mod travel;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    match std::env::args().nth(1).as_deref().unwrap_or("") {
        "autowork" => autowork::run(),
        "bandwidth" => bandwidth::run(),
        "block_throughput" => block_throughput::run(),
        "economy" => economy::run(),
        "families" => families::run(),
        "production" => production::run(),
        "travel" => travel::run(),
        _ => Err("choose a block benchmark fixture".into()),
    }
}
