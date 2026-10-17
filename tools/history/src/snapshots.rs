use super::*;
use game_sim::Host;
pub(super) fn snapshot(h: &Host, ledger: &Ledger) -> Result<Value> {
    let _measurement = game_diagnostics::span("history snapshot");
    let bodies = h.get::<game_physiology::Bodies>()?;
    let alive: Vec<_> = bodies.people.iter().filter(|(_, b)| b.alive).collect();
    let cargo = h.get::<game_cargo::Cargo>()?;
    let mut actions = BTreeMap::new();
    for a in h.get::<game_sim::Actions>()?.0.values() {
        *actions.entry(a.kind.to_string()).or_insert(0u64) += 1;
    }
    let mut stocks = BTreeMap::new();
    for i in cargo.0.values() {
        for (good, n) in &i.goods {
            *stocks
                .entry(h.get::<game_objects::Objects>()?.by_id(*good)?.name.clone())
                .or_insert(0u64) += *n;
        }
    }
    let mut plots = BTreeMap::new();
    let mut ripe = 0;
    for p in h.get::<game_agriculture::Farms>()?.plots.values() {
        *plots.entry(p.crop.clone()).or_insert(0u64) += 1;
        ripe += u64::from(matches!(p.phase, game_agriculture::Phase::Ripe { .. }));
    }
    let mut leisure = BTreeMap::new();
    for rows in h.get::<game_pastimes::Pastimes>()?.experiences.values() {
        for (k, n) in rows {
            *leisure.entry(k.clone()).or_insert(0u64) += *n;
        }
    }
    let mut areas = vec![];
    for (id, area) in &h.get::<game_land::Land>()?.0 {
        let residents: Vec<_> = alive
            .iter()
            .filter(|(p, _)| game_transport::area(h.get().unwrap(), **p) == Ok(*id))
            .collect();
        let climate = h
            .get::<game_environment::Regions>()?
            .0
            .get(id)
            .unwrap_or(h.get::<game_environment::Climate>()?)
            .at(h.now())?;
        areas.push(json!({"id":id,"name":area.name,"alive":residents.len(),"injured":residents.iter().filter(|(_,b)|b.health<b.maximum(&bodies.rules)).count(),"warmth":climate.warmth,"rain":climate.rain}));
    }
    let groups:Vec<_>=h.get::<game_institutions::Institutions>()?.groups.iter().map(|(id,g)|json!({"id":id,"name":g.name,"members":g.members,"living_members":g.members.iter().filter(|p|bodies.people.get(p).is_some_and(|b|b.alive)).count(),"procedure":g.procedure})).collect();
    let knowledge = h.get::<game_knowledge::Knowledge>()?;
    let known_areas: Vec<_> = alive
        .iter()
        .map(|(p, _)| {
            knowledge
                .0
                .get(p)
                .map_or(0, |f| game_knowledge::with_prefix(f, "roads:").count())
        })
        .collect();
    let social = h.get::<game_relationships::Social>()?;
    Ok(
        json!({"day":h.now().0.saturating_sub(ledger.epoch)/DAY.0,"time":h.now(),"alive":alive.len(),"mortals":alive.iter().filter(|(_,b)|b.kind==game_physiology::Kind::Mortal).count(),"injured":alive.iter().filter(|(_,b)|b.health<b.maximum(&bodies.rules)).count(),"hungry":alive.iter().filter(|(_,b)|b.hunger>=bodies.rules.deprivation).count(),"actions":actions,
        "goals":h.get::<game_livelihoods::Livelihoods>()?.achieved.values().sum::<u64>(),"work_failures":h.get::<game_livelihoods::Livelihoods>()?.failures.values().sum::<u64>(),"plots":plots,"ripe_plots":ripe,"pastimes":leisure,"care":h.get::<game_care::Care>()?.treated.values().sum::<u64>(),"pregnancies":h.get::<game_lifecycle::Population>()?.pregnancies.len(),"births":h.get::<game_lifecycle::Population>()?.families.len(),
        "generations":h.get::<game_heredity::Heredity>()?.generation.values().copied().max().unwrap_or(0),
        "named_objects":h.get::<game_items::Items>()?.objects.len(),"sentimental_memories":h.get::<game_sentiment::Sentiment>()?.people.values().map(|r|r.len()).sum::<usize>(),"sentimental_losses":h.get::<game_sentiment::Sentiment>()?.witnessed_losses,"acceptance_memories":h.get::<game_demand::Demand>()?.people.len(),
        "plant_lineages":h.get::<game_cultivars::Cultivars>()?.sequence,
        "wild_plants":h.get::<game_botany::Botany>()?.stands.iter().map(|(id,s)| json!({"patch":id,"living":s.plants.len(),"seeds":s.seedbank.iter().map(|l|l.quantity).sum::<u64>(),"generation":s.plants.iter().map(|p|p.strain.generation).max().unwrap_or(0),"mean_cold":s.plants.iter().map(|p|i64::from(p.expression.cold)).sum::<i64>()/s.plants.len().max(1) as i64})).collect::<Vec<_>>(),
        "plant_births":h.get::<game_botany::Botany>()?.births,"plant_deaths":h.get::<game_botany::Botany>()?.deaths,
        "shelters":h.get::<game_shelter::Shelters>()?.rooms.len(),"fuel_used":h.get::<game_shelter::Shelters>()?.fuel_used,"food_stores":h.get::<game_food_storage::Stores>()?.sites.len(),
        "trade":h.get::<game_exchange::Exchange>()?.completed,"works":h.get::<game_works::Works>()?.pieces.len(),"spoiled":h.get::<game_preservation::Preservation>()?.spoiled,
        "near_full_inventories":cargo.0.values().filter(|i|i.capacity.saturating_sub(i.mass+i.reserved_mass)<32).count(),"stock":stocks,"areas":areas,"groups":groups,"known_areas":known_areas,"affectionate_ties":social.relationships.values().flat_map(|r|r.values()).filter(|r|r.affection>=30).count(),"fearful_ties":social.relationships.values().flat_map(|r|r.values()).filter(|r|r.fear>=40).count(),"ledger":ledger.value()}),
    )
}
pub(super) fn people(h: &Host) -> Result<Vec<Value>> {
    let _measurement = game_diagnostics::span("history people");
    let social = h.get::<game_social_life::SocialLife>()?;
    h.get::<game_physiology::Bodies>()?.people.keys().map(|p| {
        let mut skills:Vec<_>=h.get::<game_skills::Skills>()?.levels.get(p).into_iter().flat_map(|s|s.iter()).collect();skills.sort_by_key(|(k,n)|(std::cmp::Reverse(**n),k.to_string()));skills.truncate(8);
        Ok(json!({"id":p,"name":social.people.get(p).map(|n|&n.name),"body":h.get::<game_physiology::Bodies>()?.body(p),"attachments":h.get::<game_sentiment::Sentiment>()?.people.get(p),"acceptance":h.get::<game_demand::Demand>()?.people.get(p),"diet":h.get::<game_nutrition::Nutrition>()?.people.get(p),"thermal":h.get::<game_exposure::Exposure>()?.people.get(p),"condition":h.query(game_vitality::Assess {person:*p})?,"generation":h.get::<game_heredity::Heredity>()?.generation.get(p),"traits":h.get::<game_traits::Traits>()?.people.get(p),"genes":h.get::<game_heredity::Heredity>()?.loci.get(p),"family":h.get::<game_lifecycle::Population>()?.families.get(p),"children":h.get::<game_households::Households>()?.children.get(p),"place":game_transport::area(h.get()?,*p).ok(),"goals":h.get::<game_livelihoods::Livelihoods>()?.achieved.get(p),"values":h.get::<game_relationships::Social>()?.values.get(p),"beliefs":h.get::<game_relationships::Social>()?.beliefs.get(p),"skills":skills,"pastimes":h.get::<game_pastimes::Pastimes>()?.experiences.get(p),"lexicon":h.get::<game_communication::Communication>()?.lexicons.get(p),"conversations":social.people.get(p).map(|n|n.encounters)}))
    }).collect()
}
