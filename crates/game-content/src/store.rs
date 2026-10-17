//! One source store; feature crates own document schemas and meaning.
use super::*;
use std::{
    collections::BTreeMap,
    path::{Component, PathBuf},
};
#[derive(Debug)]
pub struct Store {
    roots: Vec<PathBuf>,
    files: BTreeMap<String, String>,
    stamps: BTreeMap<String, Stamp>,
}
impl Store {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            roots: vec![root.into()],
            files: BTreeMap::new(),
            stamps: BTreeMap::new(),
        }
    }
    pub fn standard() -> Self {
        let local = PathBuf::from("content");
        let packaged = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.join("content")));
        let root = if local.is_dir() {
            local
        } else if let Some(p) = packaged.filter(|p| p.is_dir()) {
            p
        } else {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../content")
        };
        Self::new(root)
    }
    /// Explicit developer fixtures extend production content without overriding its rules.
    pub fn fixtures() -> Self {
        let mut store = Self::standard();
        store
            .roots
            .push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tools/fixtures/content"));
        store
    }
    /// Later packs override matching paths; new filenames extend a namespace.
    pub fn overlay(&mut self, root: impl Into<PathBuf>) -> Result<(), Error> {
        if !self.files.is_empty() {
            return Err(Error::InvalidConfiguration(
                "add overlays before loading content".into(),
            ));
        }
        self.roots.push(root.into());
        Ok(())
    }
    pub fn scan(&self, namespace: &str) -> Result<Vec<String>, Error> {
        let found = self.scan_optional(namespace)?;
        if found.is_empty() {
            return Err(Error::InvalidConfiguration(format!(
                "no content in {namespace}"
            )));
        }
        Ok(found)
    }
    /// Optional namespaces, such as authored starting scenarios, may be absent.
    pub fn scan_optional(&self, namespace: &str) -> Result<Vec<String>, Error> {
        relative(namespace)?;
        let mut found = BTreeMap::new();
        for root in &self.roots {
            let directory = root.join(namespace);
            if !directory.exists() {
                continue;
            }
            for entry in std::fs::read_dir(&directory)
                .map_err(|e| Error::InvalidConfiguration(format!("{}: {e}", directory.display())))?
            {
                let path = entry
                    .map_err(|e| Error::InvalidConfiguration(e.to_string()))?
                    .path();
                if path.extension().is_some_and(|ext| ext == "toml") && path.is_file() {
                    let name = path.file_name().unwrap().to_str().ok_or_else(|| {
                        Error::InvalidConfiguration("non-UTF8 content path".into())
                    })?;
                    found.insert(format!("{namespace}/{name}"), ());
                }
            }
        }
        Ok(found.into_keys().collect())
    }
    pub fn load<T: DeserializeOwned + Serialize>(
        &mut self,
        path: &str,
    ) -> Result<Document<T>, Error> {
        relative(path)?;
        if !self.files.contains_key(path) {
            let source = self
                .roots
                .iter()
                .rev()
                .map(|root| root.join(path))
                .find(|p| p.is_file())
                .ok_or_else(|| Error::InvalidConfiguration(format!("missing content {path}")))?;
            let text = std::fs::read_to_string(&source)
                .map_err(|e| Error::InvalidConfiguration(format!("{}: {e}", source.display())))?;
            self.files.insert(path.into(), text);
        }
        let document: Document<T> = parse(&self.files[path])
            .map_err(|e| Error::InvalidConfiguration(format!("{path}: {e}")))?;
        let stamp = document.stamp()?;
        if let Some(prior) = self.stamps.get(&stamp.id)
            && prior != &stamp
        {
            return Err(Error::InvalidConfiguration(format!(
                "conflicting content ID {}",
                stamp.id
            )));
        }
        self.stamps.insert(stamp.id.clone(), stamp);
        Ok(document)
    }
    pub fn stamps(&self) -> Vec<Stamp> {
        self.stamps.values().cloned().collect()
    }
}
impl Default for Store {
    fn default() -> Self {
        Self::standard()
    }
}
fn relative(path: &str) -> Result<(), Error> {
    if path.is_empty()
        || Path::new(path)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(Error::InvalidConfiguration(format!(
            "content path must be relative: {path}"
        )));
    }
    Ok(())
}
