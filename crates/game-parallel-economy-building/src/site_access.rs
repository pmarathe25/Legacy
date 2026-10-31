//! Physical capability indexed by each person's response to site assertions.
use game_parallel_economy_claims::Index as Claims;
use game_parallel_work_sites::{Facilities, Requirement};
use rayon::prelude::*;

pub struct Index<'a> {
    requirements: &'a [Requirement],
    claims: Option<&'a Claims<'a>>,
    any: Vec<(u64, &'a str, u64)>,
    open: Vec<(u64, &'a str, u64)>,
    owned: Vec<(u64, &'a str, u64, u64)>,
}
impl<'a> Index<'a> {
    pub fn new(sites: &'a Facilities, claims: Option<&'a Claims<'a>>) -> Self {
        let classes = claims.map_or_else(Vec::new, Claims::site_classes);
        let mut any: Vec<_> = sites
            .sites
            .par_iter()
            .map(|s| (s.region, s.kind.as_str(), s.capacity))
            .collect();
        any.par_sort_unstable();
        if classes.is_empty() {
            return Self {
                requirements: &sites.requirements,
                claims: None,
                any,
                open: Vec::new(),
                owned: Vec::new(),
            };
        }
        let mut open: Vec<_> = sites
            .sites
            .par_iter()
            .filter(|s| classes.binary_search_by_key(&s.id, |row| row.0).is_err())
            .map(|s| (s.region, s.kind.as_str(), s.capacity))
            .collect();
        open.par_sort_unstable();
        let mut owned: Vec<_> = sites
            .sites
            .par_iter()
            .filter_map(|s| {
                let owner = classes
                    .binary_search_by_key(&s.id, |row| row.0)
                    .ok()
                    .and_then(|i| classes[i].1)?;
                Some((s.region, s.kind.as_str(), owner, s.capacity))
            })
            .collect();
        owned.par_sort_unstable();
        Self {
            requirements: &sites.requirements,
            claims,
            any,
            open,
            owned,
        }
    }
    pub fn permits(&self, person: u64, region: u64, recipe: u32) -> bool {
        let Ok(i) = self
            .requirements
            .binary_search_by_key(&recipe, |r| r.recipe)
        else {
            return true;
        };
        let requirement = &self.requirements[i];
        let key = (region, requirement.kind.as_str(), requirement.slots);
        let matches = |sites: &[(u64, &str, u64)]| {
            let i = sites.partition_point(|site| *site < key);
            sites
                .get(i)
                .is_some_and(|s| s.0 == region && s.1 == requirement.kind)
        };
        if self.claims.is_none_or(|claims| claims.disregards(person)) {
            return matches(&self.any);
        }
        if matches(&self.open) {
            return true;
        }
        let key = (region, requirement.kind.as_str(), person, requirement.slots);
        let i = self.owned.partition_point(|site| *site < key);
        self.owned
            .get(i)
            .is_some_and(|s| s.0 == region && s.1 == requirement.kind && s.2 == person)
    }
}
