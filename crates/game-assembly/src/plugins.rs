use game_core::Result;
use game_sim::Builder;
pub fn builder() -> Result<Builder> {
    let mut b = Builder::new();
    game_land::plugin(&mut b)?;
    game_transport::plugin(&mut b)?;
    game_objects::plugin(&mut b)?;
    game_traits::plugin(&mut b)?;
    game_vitality::plugin(&mut b)?;
    game_nutrition::plugin(&mut b)?;
    game_cargo::plugin(&mut b)?;
    game_cultivars::plugin(&mut b)?;
    game_botany::plugin(&mut b)?;
    game_property::plugin(&mut b)?;
    game_environment::plugin(&mut b)?;
    game_physiology::plugin(&mut b)?;
    game_communication::plugin(&mut b)?;
    game_knowledge::plugin(&mut b)?;
    game_skills::plugin(&mut b)?;
    game_ecology::plugin(&mut b)?;
    game_agriculture::plugin(&mut b)?;
    game_workshop::plugin(&mut b)?;
    game_health::plugin(&mut b)?;
    game_relationships::plugin(&mut b)?;
    game_agreements::plugin(&mut b)?;
    game_institutions::plugin(&mut b)?;
    game_justice::plugin(&mut b)?;
    game_lifecycle::plugin(&mut b)?;
    game_lineage::plugin(&mut b)?;
    game_guardians::plugin(&mut b)?;
    game_invocation::plugin(&mut b)?;
    game_social_life::plugin(&mut b)?;
    game_characters::plugin(&mut b)?;
    game_names::plugin(&mut b)?;
    game_naming::plugin(&mut b)?;
    game_logistics::plugin(&mut b)?;
    game_projects::plugin(&mut b)?;
    game_realm::plugin(&mut b)?;
    game_conflict::plugin(&mut b)?;
    game_confinement::plugin(&mut b)?;
    game_missions::plugin(&mut b)?;
    game_armies::plugin(&mut b)?;
    game_pressure::plugin(&mut b)?;
    game_communities::plugin(&mut b)?;
    game_stockpiles::plugin(&mut b)?;
    game_wayfaring::plugin(&mut b)?;
    game_horticulture::plugin(&mut b)?;
    game_care::plugin(&mut b)?;
    game_livelihoods::plugin(&mut b)?;
    game_pastimes::plugin(&mut b)?;
    game_heredity::plugin(&mut b)?;
    game_households::plugin(&mut b)?;
    game_parenthood::plugin(&mut b)?;
    game_items::plugin(&mut b)?;
    game_sentiment::plugin(&mut b)?;
    game_demand::plugin(&mut b)?;
    game_exchange::plugin(&mut b)?;
    game_works::plugin(&mut b)?;
    game_preservation::plugin(&mut b)?;
    game_food_storage::plugin(&mut b)?;
    game_maturation::plugin(&mut b)?;
    game_equipment::plugin(&mut b)?;
    game_shelter::plugin(&mut b)?;
    game_exposure::plugin(&mut b)?;
    game_affiliation::plugin(&mut b)?;
    game_discourse::plugin(&mut b)?;
    game_grievances::plugin(&mut b)?;
    game_campaign::plugin(&mut b)?;
    game_wellbeing::plugin(&mut b)?;
    game_openings::plugin(&mut b)?;
    crate::binding::register(&mut b)?;
    Ok(b)
}

/// Introduced owners have empty, inactive state in earlier worlds.
pub fn load(saved: &str, workers: usize) -> Result<game_sim::Host> {
    #[derive(serde::Deserialize)]
    struct Header {
        plugins: std::collections::BTreeMap<String, u32>,
    }
    let header: Header =
        serde_json::from_str(saved).map_err(|e| game_core::Error::InvalidSave(e.to_string()))?;
    if header.plugins.contains_key("communication") && header.plugins.contains_key("discourse") {
        return builder()?.load(saved, workers);
    }
    let mut value: serde_json::Value =
        serde_json::from_str(saved).map_err(|e| game_core::Error::InvalidSave(e.to_string()))?;
    if !header.plugins.contains_key("communication") {
        game_core::require(
            value["resources"].get("communication.experience").is_none(),
            "undeclared communication state",
        )?;
        value["plugins"]["communication"] = serde_json::json!(1);
        value["resources"]["communication.experience"] =
            serde_json::to_value(game_communication::Communication::default()).unwrap();
    }
    if !header.plugins.contains_key("discourse") {
        value["plugins"]["discourse"] = serde_json::json!(1);
    }
    builder()?.load(&serde_json::to_string(&value).unwrap(), workers)
}
