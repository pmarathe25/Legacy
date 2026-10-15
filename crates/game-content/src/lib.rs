//! Reads authored definitions. The receiving feature validates their meaning.
mod store;
use game_core::Error;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::path::Path;
pub use store::Store;

/// Startup configuration chooses a file or an explicit authored snapshot.
/// Resolving a source never changes definitions in an already running world.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case", deny_unknown_fields)]
pub enum DefinitionSource<T> {
    File { path: std::path::PathBuf },
    Inline { document: Document<T> },
}
impl<T: DeserializeOwned + Clone> DefinitionSource<T> {
    pub fn resolve(&self) -> Result<Document<T>, Error> {
        match self {
            Self::File { path } => read(path),
            Self::Inline { document } => {
                document.validate_header()?;
                Ok(document.clone())
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document<T> {
    pub id: String,
    pub version: u32,
    pub data: T,
}

impl<T> Document<T> {
    pub fn validate_header(&self) -> Result<(), Error> {
        if self.version == 0
            || self.id.is_empty()
            || !self
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-/".contains(&b))
        {
            return Err(Error::InvalidConfiguration(
                "invalid content ID or version".into(),
            ));
        }
        Ok(())
    }
}

/// Provenance is saved beside the feature's full definitions. This fingerprint
/// detects ordinary edits; it is not a cryptographic authenticity check.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stamp {
    pub id: String,
    pub version: u32,
    pub fingerprint: u64,
}

impl<T: Serialize> Document<T> {
    pub fn stamp(&self) -> Result<Stamp, Error> {
        self.validate_header()?;
        let bytes = serde_json::to_vec(self)
            .map_err(|e| Error::InvalidConfiguration(format!("invalid content: {e}")))?;
        let fingerprint = bytes
            .into_iter()
            .fold(0xcbf29ce484222325_u64, |hash, byte| {
                (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
            });
        Ok(Stamp {
            id: self.id.clone(),
            version: self.version,
            fingerprint,
        })
    }
}

pub fn parse<T: DeserializeOwned>(source: &str) -> Result<Document<T>, Error> {
    let document: Document<T> = toml::from_str(source)
        .map_err(|e| Error::InvalidConfiguration(format!("invalid TOML content: {e}")))?;
    document.validate_header()?;
    Ok(document)
}

pub fn read<T: DeserializeOwned>(path: impl AsRef<Path>) -> Result<Document<T>, Error> {
    let path = path.as_ref();
    let source = std::fs::read_to_string(path)
        .map_err(|e| Error::InvalidConfiguration(format!("{}: {e}", path.display())))?;
    parse(&source).map_err(|e| match e {
        Error::InvalidConfiguration(reason) => {
            Error::InvalidConfiguration(format!("{}: {reason}", path.display()))
        }
        other => other,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Content {
        amount: u64,
        labels: Vec<String>,
    }

    #[test]
    fn comments_and_layout_do_not_change_definition_fingerprint() {
        let a =
            parse::<Content>("id='test/stock'\nversion=1\n[data]\namount=250\nlabels=['a','b']")
                .unwrap();
        let b = parse::<Content>("# editable content\nid = 'test/stock'\nversion = 1\n[data]\nlabels = ['a', 'b']\namount = 250 # native units").unwrap();
        assert_eq!(a, b);
        assert_eq!(a.stamp(), b.stamp());
        let changed =
            parse::<Content>("id='test/stock'\nversion=1\n[data]\namount=500\nlabels=['a','b']")
                .unwrap();
        assert_ne!(a.stamp(), changed.stamp());
    }

    #[test]
    fn unknown_fields_and_invalid_headers_fail() {
        for source in [
            "id='test'\nversion=0\n[data]\namount=1\nlabels=[]",
            "id='../bad space'\nversion=1\n[data]\namount=1\nlabels=[]",
            "id='test'\nversion=1\nextra=2\n[data]\namount=1\nlabels=[]",
            "id='test'\nversion=1\n[data]\namount=1\nlabels=[]\ntypo=true",
        ] {
            assert!(parse::<Content>(source).is_err());
        }
    }
}
