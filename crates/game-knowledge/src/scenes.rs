use super::*;
use game_storage::Version;
#[derive(Clone, Debug)]
pub(super) struct CachedScene {
    land: Version<AreaId, Arc<game_land::Area>>,
    ecology: Option<Version<u64, game_ecology::Patch>>,
    scene: Arc<game_facts::Scene<FactValue>>,
}
pub(super) fn visible(
    tx: &mut Transaction<'_>,
    area: AreaId,
) -> Result<Arc<game_facts::Scene<FactValue>>> {
    let land = &tx.get::<game_land::Land>()?.0;
    let ecology = tx.get::<game_ecology::Ecology>().ok();
    let previous = tx.get::<Perception>()?.scenes.get(&area);
    if let Some(old) = previous
        && land.matches(&old.land)
        && match (ecology, &old.ecology) {
            (Some(world), Some(version)) => version.matches(&world.patches),
            (None, None) => true,
            _ => false,
        }
    {
        return Ok(old.scene.clone());
    }
    let roads = land
        .shared(&area)
        .ok_or_else(|| Error::Missing("area".into()))?;
    let mut entries = vec![(roads_key(area).into(), Arc::new(FactValue::Roads(roads)))];
    if let Some(world) = ecology {
        for id in world.in_area(area) {
            let patch = world
                .patches
                .get(&id)
                .ok_or_else(|| Error::Missing("patch".into()))?;
            require(patch.area == area, "cannot observe distant patch")?;
            entries.push((
                patch_key(id).into(),
                Arc::new(FactValue::Patch(patch.clone())),
            ));
        }
    }
    let scene = Arc::new(game_facts::Scene::new(
        entries,
        previous.map(|p| p.scene.as_ref()),
    )?);
    let cached = CachedScene {
        land: land.version(),
        ecology: ecology.map(|w| w.patches.version()),
        scene: scene.clone(),
    };
    tx.edit::<Perception>()?.scenes.insert(area, cached);
    Ok(scene)
}
