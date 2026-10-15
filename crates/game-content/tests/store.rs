use game_content::*;
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "legacy-store-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(p.join("objects")).unwrap();
        Self(p)
    }
    fn write(&self, file: &str, value: u64) {
        std::fs::write(
            self.0.join(file),
            format!("id='test/{file}'\nversion=1\n[data]\namount={value}"),
        )
        .unwrap();
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Data {
    amount: u64,
}
#[test]
fn overlays_override_paths_and_add_new_documents_in_stable_order() {
    let base = Temp::new();
    let overlay = Temp::new();
    base.write("objects/base.toml", 1);
    overlay.write("objects/base.toml", 2);
    overlay.write("objects/new.toml", 3);
    let mut store = Store::new(&base.0);
    store.overlay(&overlay.0).unwrap();
    assert_eq!(
        store.scan("objects").unwrap(),
        vec!["objects/base.toml", "objects/new.toml"]
    );
    assert_eq!(
        store.load::<Data>("objects/base.toml").unwrap().data.amount,
        2
    );
    assert_eq!(
        store.load::<Data>("objects/new.toml").unwrap().data.amount,
        3
    );
    assert_eq!(store.stamps().len(), 2);
    assert!(store.overlay(&base.0).is_err());
}
#[test]
fn a_store_freezes_loaded_documents_and_a_new_store_sees_edits() {
    let root = Temp::new();
    root.write("objects/base.toml", 1);
    let mut store = Store::new(&root.0);
    assert_eq!(
        store.load::<Data>("objects/base.toml").unwrap().data.amount,
        1
    );
    root.write("objects/base.toml", 2);
    assert_eq!(
        store.load::<Data>("objects/base.toml").unwrap().data.amount,
        1
    );
    assert_eq!(
        Store::new(&root.0)
            .load::<Data>("objects/base.toml")
            .unwrap()
            .data
            .amount,
        2
    );
    assert!(store.load::<Data>("../outside.toml").is_err());
}
