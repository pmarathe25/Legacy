use super::*;
use serde::ser::SerializeMap;
#[derive(Deserialize)]
pub(super) struct Saved {
    people: Packed<PersonId, Body>,
    rules: Rules,
    ticking: bool,
}
impl From<Saved> for Bodies {
    fn from(saved: Saved) -> Self {
        let active = saved
            .people
            .iter()
            .filter(|(_, b)| b.alive)
            .map(|(id, _)| (*id, ()))
            .collect();
        let clock = saved
            .people
            .values()
            .map(|b| b.last)
            .min()
            .unwrap_or_default();
        Self {
            people: saved.people,
            rules: saved.rules,
            ticking: saved.ticking,
            active,
            clock,
        }
    }
}
impl Bodies {
    pub fn body(&self, person: &PersonId) -> Option<Body> {
        self.people.get(person).map(|b| self.logical(*b))
    }
    pub fn living(&self) -> impl Iterator<Item = (&PersonId, &Body)> {
        self.active.keys().map(|id| (id, &self.people[id]))
    }
    fn logical(&self, mut body: Body) -> Body {
        if !body.alive {
            body = advance_body(&body, &self.rules, self.clock);
        }
        body
    }
}
impl PartialEq for Bodies {
    fn eq(&self, other: &Self) -> bool {
        self.rules == other.rules
            && self.ticking == other.ticking
            && self
                .people
                .iter()
                .map(|(id, b)| (id, self.logical(*b)))
                .eq(other.people.iter().map(|(id, b)| (id, other.logical(*b))))
    }
}
impl Eq for Bodies {}
struct Rows<'a>(&'a Bodies);
impl Serialize for Rows<'_> {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.people.len()))?;
        for (id, body) in self.0.people.iter() {
            map.serialize_entry(id, &self.0.logical(*body))?;
        }
        map.end()
    }
}
impl Serialize for Bodies {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct View<'a> {
            people: Rows<'a>,
            rules: &'a Rules,
            ticking: bool,
        }
        View {
            people: Rows(self),
            rules: &self.rules,
            ticking: self.ticking,
        }
        .serialize(serializer)
    }
}
