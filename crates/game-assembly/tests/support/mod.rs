use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
pub struct Pack {
    pub root: PathBuf,
}
impl Pack {
    pub fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "legacy-content-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        copy(&source, &root);
        Self { root }
    }
    pub fn edit(&self, path: &str, from: &str, to: &str) {
        let p = self.root.join(path);
        let text = std::fs::read_to_string(&p).unwrap();
        assert!(text.contains(from), "missing replacement in {path}");
        std::fs::write(p, text.replace(from, to)).unwrap();
    }
    pub fn write(&self, path: &str, text: &str) {
        let p = self.root.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
}
fn copy(source: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let path = entry.unwrap().path();
        let target = to.join(path.file_name().unwrap());
        if path.is_dir() {
            copy(&path, &target);
        } else {
            std::fs::copy(path, target).unwrap();
        }
    }
}
impl Drop for Pack {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
