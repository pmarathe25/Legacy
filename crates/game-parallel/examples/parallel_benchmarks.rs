//! Run the named throughput fixture; remaining arguments belong to that fixture.
#[path = "support/autonomy_throughput.rs"]
mod autonomy_throughput;
#[path = "support/autowork_throughput.rs"]
mod autowork_throughput;
#[path = "support/economy_throughput.rs"]
mod economy_throughput;
#[path = "support/family_throughput.rs"]
mod family_throughput;
#[path = "support/farm_throughput.rs"]
mod farm_throughput;
#[path = "support/goods_transfer_throughput.rs"]
mod goods_transfer_throughput;
#[path = "support/market_auto_throughput.rs"]
mod market_auto_throughput;
#[path = "support/market_throughput.rs"]
mod market_throughput;
#[path = "support/packing_throughput.rs"]
mod packing_throughput;
#[path = "support/production_start_throughput.rs"]
mod production_start_throughput;
#[path = "support/production_throughput.rs"]
mod production_throughput;
#[path = "support/shipment_throughput.rs"]
mod shipment_throughput;
#[path = "support/social_throughput.rs"]
mod social_throughput;
#[path = "support/travel_throughput.rs"]
mod travel_throughput;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    match std::env::args().nth(1).as_deref().unwrap_or("") {
        "autonomy_throughput" => autonomy_throughput::run(),
        "autowork_throughput" => autowork_throughput::run(),
        "economy_throughput" => economy_throughput::run(),
        "family_throughput" => family_throughput::run(),
        "farm_throughput" => farm_throughput::run(),
        "goods_transfer_throughput" => goods_transfer_throughput::run(),
        "market_auto_throughput" => market_auto_throughput::run(),
        "market_throughput" => market_throughput::run(),
        "packing_throughput" => packing_throughput::run(),
        "production_start_throughput" => production_start_throughput::run(),
        "production_throughput" => production_throughput::run(),
        "shipment_throughput" => shipment_throughput::run(),
        "social_throughput" => social_throughput::run(),
        "travel_throughput" => travel_throughput::run(),
        _ => Err("choose a documented throughput fixture".into()),
    }
}
