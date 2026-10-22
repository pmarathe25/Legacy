use super::*;
impl<V: Content> Serialize for Memories<V> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let archive =
            game_snapshot::evidence::View::new(self.0.iter().map(|(p, f)| (*p, f.as_facts())));
        let mut map = s.serialize_map(Some(2))?;
        map.serialize_entry("archive", &archive)?;
        map.serialize_entry("cognition", &self.1)?;
        map.end()
    }
}
impl<'de, V: Content> Deserialize<'de> for Memories<V> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let parts = game_snapshot::evidence::Parts::<V, Table<PersonId, Book>>::deserialize(d)?;
        let legacy = parts.metadata.is_none();
        let mut state = Self(Default::default(), parts.metadata.unwrap_or_default());
        for (p, f) in &parts.facts {
            state.0.insert(*p, Facts::from_facts(f.clone()));
        }
        if legacy {
            for (p, facts) in state.0.iter() {
                let entries = facts
                    .iter()
                    .map(|(k, f)| (k.clone(), Trace::legacy(f.observed, f.value.cues(k))))
                    .collect();
                state.1.insert(
                    *p,
                    Book {
                        entries,
                        sweep: None,
                        due: None,
                    },
                );
            }
        }
        Ok(state)
    }
}
