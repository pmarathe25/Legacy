//! Integer calendar and authored periodic climate.
use game_core::*;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Regions(pub Table<AreaId, Climate>);
impl Resource for Regions {
    const KEY: &'static str = "environment.regions";
    fn validate(&self) -> Result<()> {
        for climate in self.0.values() {
            climate.validate()?;
            require(!climate.seasons.is_empty(), "empty regional climate")?;
        }
        Ok(())
    }
    fn validate_changes(&self, old: &Self) -> Result<()> {
        for (_, climate) in self.0.changed(&old.0) {
            climate.validate()?;
            require(!climate.seasons.is_empty(), "empty regional climate")?;
        }
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct ConfigureArea {
    pub area: AreaId,
    pub climate: Climate,
}
impl Request for ConfigureArea {
    const KEY: &'static str = "environment.configure-area";
    type Response = ();
}
pub fn at<'a>(tx: &'a Transaction<'_>, area: AreaId) -> Result<&'a Season> {
    if let Some(climate) = tx.get::<Regions>()?.0.get(&area) {
        climate.at(tx.now())
    } else {
        tx.get::<Climate>()?.at(tx.now())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Season {
    pub duration: SimDuration,
    pub warmth: i32,
    pub rain: u32,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Climate {
    pub seasons: Vec<Season>,
}
impl Resource for Climate {
    const KEY: &'static str = "environment.climate";
    fn validate(&self) -> Result<()> {
        for s in &self.seasons {
            require(s.duration.0 > 0, "season must have duration")?;
        }
        self.year()?;
        Ok(())
    }
}
impl Climate {
    pub fn year(&self) -> Result<u64> {
        self.seasons.iter().try_fold(0u64, |q, s| {
            q.checked_add(s.duration.0).ok_or(Error::Overflow)
        })
    }
    pub fn at(&self, time: SimTime) -> Result<&Season> {
        let year = self.year()?;
        require(year > 0, "climate has not been configured")?;
        let mut t = time.0 % year;
        for season in &self.seasons {
            if t < season.duration.0 {
                return Ok(season);
            }
            t -= season.duration.0;
        }
        unreachable!("positive checked year")
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Configure(pub Climate);
impl Request for Configure {
    const KEY: &'static str = "environment.configure";
    type Response = ();
}
pub fn load(store: &mut game_content::Store, key: &str) -> Result<Climate> {
    let doc: game_content::Document<Climate> = store.load(&format!("environment/{key}.toml"))?;
    doc.data.validate()?;
    require(!doc.data.seasons.is_empty(), "no seasons")?;
    Ok(doc.data)
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("environment", 1)?;
    b.resource::<Climate>()?;
    b.resource::<Regions>()?;
    b.request::<ConfigureArea>(|tx, r| {
        r.climate.validate()?;
        require(!r.climate.seasons.is_empty(), "empty regional climate")?;
        tx.edit::<Regions>()?.0.insert(r.area, r.climate);
        Ok(())
    })?;
    b.request::<Configure>(|tx, r| {
        r.0.validate()?;
        require(!r.0.seasons.is_empty(), "no seasons")?;
        *tx.edit::<Climate>()? = r.0;
        Ok(())
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn climate_uses_exact_boundaries() {
        let c = Climate {
            seasons: vec![
                Season {
                    duration: SimDuration(10),
                    warmth: 5,
                    rain: 2,
                },
                Season {
                    duration: SimDuration(20),
                    warmth: 1,
                    rain: 0,
                },
            ],
        };
        assert_eq!(c.at(SimTime(9)).unwrap().warmth, 5);
        assert_eq!(c.at(SimTime(10)).unwrap().warmth, 1);
        assert_eq!(c.at(SimTime(30)).unwrap().warmth, 5);
    }
}
