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
    game_syntax::plugin(&mut b)?;
    game_knowledge::plugin(&mut b)?;
    game_testimony::plugin(&mut b)?;
    game_information::plugin(&mut b)?;
    game_skills::plugin(&mut b)?;
    game_ecology::plugin(&mut b)?;
    game_agriculture::plugin(&mut b)?;
    game_workshop::plugin(&mut b)?;
    game_mechanics::plugin(&mut b)?;
    game_invention::plugin(&mut b)?;
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
    game_companionship::plugin(&mut b)?;
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
    game_search::plugin(&mut b)?;
    game_care::plugin(&mut b)?;
    game_livelihoods::plugin(&mut b)?;
    game_pastimes::plugin(&mut b)?;
    game_heredity::plugin(&mut b)?;
    game_households::plugin(&mut b)?;
    game_family_life::plugin(&mut b)?;
    game_parenthood::plugin(&mut b)?;
    game_items::plugin(&mut b)?;
    game_sentiment::plugin(&mut b)?;
    game_attachments::plugin(&mut b)?;
    game_demand::plugin(&mut b)?;
    game_exchange::plugin(&mut b)?;
    game_credit::plugin(&mut b)?;
    game_works::plugin(&mut b)?;
    game_inquiry::plugin(&mut b)?;
    game_observations::plugin(&mut b)?;
    game_preservation::plugin(&mut b)?;
    game_food_storage::plugin(&mut b)?;
    game_maturation::plugin(&mut b)?;
    game_equipment::plugin(&mut b)?;
    game_shelter::plugin(&mut b)?;
    game_comfort::plugin(&mut b)?;
    game_exposure::plugin(&mut b)?;
    game_affiliation::plugin(&mut b)?;
    game_discourse::plugin(&mut b)?;
    game_grievances::plugin(&mut b)?;
    game_campaign::plugin(&mut b)?;
    game_wellbeing::plugin(&mut b)?;
    game_openings::plugin(&mut b)?;
    crate::binding::register(&mut b)?;
    game_autonomy_frames::register(&mut b)?;
    Ok(b)
}
pub fn load(saved: &str, workers: usize) -> Result<game_sim::Host> {
    let normalized = game_save_compat::normalize(saved, || builder()?.build(0, 1)?.save())?;
    builder()?.load(&normalized, workers)
}
/// Current checkpoints decode one owned resource at a time; legacy owners use the existing migration.
pub fn load_from(
    source: &mut (impl std::io::Read + std::io::Seek),
    workers: usize,
) -> Result<game_sim::Host> {
    let io = |e: std::io::Error| game_core::Error::InvalidSave(e.to_string());
    let start = source.stream_position().map_err(io)?;
    match builder()?.load_from(&mut *source, workers) {
        Err(error)
            if matches!(&error, game_core::Error::Rejected(message) if message == "incompatible save manifest")
                || matches!(&error, game_core::Error::InvalidSave(message) if message.starts_with("unknown resource ")) =>
        {
            source.seek(std::io::SeekFrom::Start(start)).map_err(io)?;
            let mut saved = String::new();
            source.read_to_string(&mut saved).map_err(io)?;
            load(&saved, workers)
        }
        result => result,
    }
}
