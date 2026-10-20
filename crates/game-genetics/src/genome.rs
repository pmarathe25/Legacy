//! Small immutable genomes keep sorted loci in one allocation, with the same map wire format.
use crate::{Alleles, GeneName};
use serde::{Deserialize, Serialize, de::MapAccess, ser::SerializeMap};
use std::{borrow::Borrow, ops::Index};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Genome(Box<[(GeneName, Alleles)]>);
impl Genome {
    pub fn get<Q: Ord + ?Sized>(&self, key: &Q) -> Option<&Alleles>
    where
        GeneName: Borrow<Q>,
    {
        self.0
            .binary_search_by(|(name, _)| name.borrow().cmp(key))
            .ok()
            .map(|i| &self.0[i].1)
    }
    pub fn insert(&mut self, key: GeneName, value: Alleles) -> Option<Alleles> {
        match self.0.binary_search_by(|(name, _)| name.cmp(&key)) {
            Ok(i) => Some(std::mem::replace(&mut self.0[i].1, value)),
            Err(i) => {
                let mut loci = std::mem::take(&mut self.0).into_vec();
                loci.insert(i, (key, value));
                self.0 = loci.into_boxed_slice();
                None
            }
        }
    }
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&GeneName, &Alleles)> {
        self.0.iter().map(|(key, value)| (key, value))
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
impl<'a> IntoIterator for &'a Genome {
    type Item = (&'a GeneName, &'a Alleles);
    type IntoIter = std::iter::Map<
        std::slice::Iter<'a, (GeneName, Alleles)>,
        fn(&'a (GeneName, Alleles)) -> Self::Item,
    >;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter().map(|(key, value)| (key, value))
    }
}
impl<Q: Ord + ?Sized> Index<&Q> for Genome
where
    GeneName: Borrow<Q>,
{
    type Output = Alleles;
    fn index(&self, key: &Q) -> &Alleles {
        self.get(key).expect("unknown locus")
    }
}
impl FromIterator<(GeneName, Alleles)> for Genome {
    fn from_iter<T: IntoIterator<Item = (GeneName, Alleles)>>(rows: T) -> Self {
        let mut rows: Vec<_> = rows.into_iter().collect();
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        // Match ordered-map construction: the last occurrence of a key wins.
        rows.reverse();
        rows.dedup_by(|a, b| a.0 == b.0);
        rows.reverse();
        Self(rows.into_boxed_slice())
    }
}
impl<const N: usize> From<[(GeneName, Alleles); N]> for Genome {
    fn from(rows: [(GeneName, Alleles); N]) -> Self {
        rows.into_iter().collect()
    }
}
impl Serialize for Genome {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(self.len()))?;
        for (key, value) in self {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}
impl<'de> Deserialize<'de> for Genome {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Loci;
        impl<'de> serde::de::Visitor<'de> for Loci {
            type Value = Genome;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("named diploid loci")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Genome, A::Error> {
                let mut rows = Vec::new();
                while let Some(row) = map.next_entry()? {
                    rows.push(row);
                }
                Ok(rows.into_iter().collect())
            }
        }
        d.deserialize_map(Loci)
    }
}
