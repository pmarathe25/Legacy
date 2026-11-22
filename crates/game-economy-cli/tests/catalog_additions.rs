use game_economy::core_interval::CoreHost;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

struct ContentCopy(PathBuf);

impl ContentCopy {
    fn new(source: &Path) -> Self {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        let path = std::env::temp_dir().join(format!(
            "legacy-catalog-{}-{}",
            std::process::id(),
            now.as_nanos()
        ));
        fs::create_dir(&path).unwrap();
        let copy = Self(path);
        for category in ["objects", "workshop"] {
            let destination = copy.0.join(category);
            fs::create_dir(&destination).unwrap();
            for entry in fs::read_dir(source.join(category)).unwrap() {
                let entry = entry.unwrap();
                if entry.path().extension().is_some_and(|ext| ext == "toml") {
                    fs::copy(entry.path(), destination.join(entry.file_name())).unwrap();
                }
            }
        }
        copy
    }
}

impl Drop for ContentCopy {
    fn drop(&mut self) {
        for category in ["objects", "workshop"] {
            let directory = self.0.join(category);
            if let Ok(entries) = fs::read_dir(&directory) {
                for entry in entries.flatten() {
                    let _ = fs::remove_file(entry.path());
                }
                let _ = fs::remove_dir(directory);
            }
        }
        let _ = fs::remove_dir(&self.0);
    }
}

#[test]
fn a_saved_world_can_add_packs_without_changing_existing_physics() {
    let content = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let scenario = content.join("scenarios/economy-sparse-regional-storage.toml");
    let mut host = game_economy_cli::core_scenario(&scenario, &content).unwrap();
    let opening = host.snapshot();
    let fixture = opening
        .core
        .physical
        .recipes
        .iter()
        .find(|recipe| recipe.name == "fixture-tool")
        .unwrap()
        .id;
    assert!(host.start_controlled_work(1, fixture, 1).unwrap().1);
    assert_eq!(host.advance_continuous(17, &[]).unwrap().failure, None);
    let before = host.snapshot();
    let saved = serde_json::to_vec(&before).unwrap();
    let host = CoreHost::resume(serde_json::from_slice(&saved).unwrap()).unwrap();
    let copy = ContentCopy::new(&content);
    fs::write(
        copy.0.join("objects/woodcraft.toml"),
        r#"id = "objects/woodcraft"
version = 1
namespace = "woodcraft"
requires = { "objects/base" = 1 }
[data.objects.carved-pin]
id = 1
runtime_id = 7700001
name = "Carved pin"
"#,
    )
    .unwrap();
    fs::write(
        copy.0.join("workshop/woodcraft.toml"),
        r#"id = "workshop/woodcraft"
version = 1
namespace = "woodcraft"
requires = { "objects/woodcraft" = 1 }
[data.recipes.carve-pin]
runtime_id = 7700002
inputs = { timber = 1 }
output = "carved-pin"
quantity = 1
duration = 60000
skill = "craft"
"#,
    )
    .unwrap();
    let updated = game_economy_cli::extend_catalog(&host, &copy.0).unwrap();
    let mut after = updated.snapshot();
    let physical = &mut after.core.physical;
    let good = physical.goods.pop().unwrap();
    let recipe = physical.recipes.pop().unwrap();
    assert_eq!(
        (good.id, good.name.as_str()),
        (7700001, "woodcraft:carved-pin")
    );
    assert_eq!(
        (recipe.id, recipe.name.as_str(), recipe.outputs[0].good),
        (7700002, "woodcraft:carve-pin", good.id)
    );
    let manifest = physical.catalog_manifest.as_ref().unwrap();
    assert!(
        manifest
            .packs
            .iter()
            .any(|pack| pack.id == "objects/woodcraft" && pack.requires == ["objects/base"])
    );
    physical.catalog_manifest = before.core.physical.catalog_manifest.clone();
    assert_eq!(after, before);
    assert_eq!(
        game_economy_cli::extend_catalog(&updated, &copy.0)
            .unwrap()
            .snapshot(),
        updated.snapshot()
    );
    let opening = CoreHost::resume(opening).unwrap();
    let mut crafter = game_economy_cli::extend_catalog(&opening, &copy.0).unwrap();
    let (job, started) = crafter.start_controlled_work(1, recipe.id, 1).unwrap();
    assert!(started);
    let advancement = crafter.advance_continuous(3_600_000, &[]).unwrap();
    assert_eq!(advancement.failure, None);
    assert!(
        crafter
            .snapshot()
            .core
            .physical
            .custody
            .containers
            .iter()
            .any(|holder| {
                holder.place == game_lots::Place::JobOutput(job)
                    && holder
                        .lots
                        .iter()
                        .any(|lot| lot.good == good.id && lot.quantity == 1)
            })
    );

    let base_recipe = copy.0.join("workshop/recipes.toml");
    let original_recipe = fs::read_to_string(&base_recipe).unwrap();
    for (original, change, message) in [
        (
            "timber = 5",
            "timber = 4",
            "changed or removed recipe fixture-tool",
        ),
        (
            "skill = \"craft\"",
            "skill = \"hunting\"",
            "changed skill category of recipe fixture-tool",
        ),
    ] {
        fs::write(&base_recipe, original_recipe.replace(original, change)).unwrap();
        let error = game_economy_cli::extend_catalog(&host, &copy.0)
            .err()
            .unwrap();
        assert!(error.contains(message), "{error}");
        assert_eq!(host.snapshot(), before);
    }

    let mut legacy = before;
    legacy.core.physical.catalog_manifest = None;
    let legacy = CoreHost::resume(legacy).unwrap();
    assert!(game_economy_cli::extend_catalog(&legacy, &copy.0).is_err());
}
