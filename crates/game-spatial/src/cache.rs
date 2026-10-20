//! Bounded derived paths retain namespace tokens, never personal histories or world state.
use crate::*;
use std::sync::Weak;
type Key = (usize, AreaId, AreaId);
#[derive(Default)]
pub struct Routes {
    entries: BTreeMap<Key, (Weak<()>, Result<Route>)>,
}
impl Routes {
    /// The token must change whenever any supplied road changes. An empty token
    /// is reserved for an empty graph; weak ownership prevents allocation reuse.
    pub fn route<'a>(
        &mut self,
        version: Weak<()>,
        from: AreaId,
        to: AreaId,
        area_at: impl FnMut(AreaId) -> Option<Cow<'a, Area>>,
    ) -> Result<Route> {
        let key = (version.as_ptr() as usize, from, to);
        if let Some((_, route)) = self.entries.get(&key) {
            return route.clone();
        }
        let route = route_with(from, to, area_at);
        if self.entries.len() >= 2048 {
            self.entries.retain(|_, (v, _)| v.strong_count() > 0);
            if self.entries.len() >= 2048 {
                self.entries.clear();
            }
        }
        self.entries.insert(key, (version, route.clone()));
        route
    }
}
