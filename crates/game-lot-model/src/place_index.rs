//! Verified direct lookup for ordered physical custody, with sparse fallback.
use crate::Place;

/// An immutable index over rows ordered by distinct physical place.
/// Dense identifiers are located directly; every hit is checked before use.
pub struct PlaceIndex<'a, T> {
    rows: &'a [T],
    place: fn(&T) -> Place,
    anchors: [Option<(usize, u64)>; 5],
}
fn kind(place: Place) -> Option<(usize, u64)> {
    match place {
        Place::Inventory(id) => Some((0, id)),
        Place::Person(id) => Some((1, id)),
        Place::Site(id) => Some((2, id)),
        Place::JobInput(id) => Some((3, id)),
        Place::JobOutput(id) => Some((4, id)),
        Place::Stock { .. } => None,
    }
}
impl<'a, T> PlaceIndex<'a, T> {
    pub fn new(rows: &'a [T], place: fn(&T) -> Place) -> Self {
        let probes = [
            Place::Inventory(0),
            Place::Person(0),
            Place::Site(0),
            Place::JobInput(0),
            Place::JobOutput(0),
        ];
        let anchors = std::array::from_fn(|i| {
            let start = rows.partition_point(|row| place(row) < probes[i]);
            let (actual, first) = kind(place(rows.get(start)?))?;
            (actual == i).then_some((start, first))
        });
        Self {
            rows,
            place,
            anchors,
        }
    }
    pub fn locate(&self, wanted: Place) -> Option<usize> {
        let direct = kind(wanted).and_then(|(slot, id)| {
            let (start, first) = self.anchors[slot]?;
            let offset = usize::try_from(id.checked_sub(first)?).ok()?;
            let index = start.checked_add(offset)?;
            self.rows
                .get(index)
                .filter(|row| (self.place)(row) == wanted)
                .map(|_| index)
        });
        direct.or_else(|| self.rows.binary_search_by_key(&wanted, self.place).ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lookup_handles_nonzero_starts_gaps_and_variant_boundaries() {
        let rows = [
            Place::Inventory(4),
            Place::Stock { region: 1, good: 2 },
            Place::Person(10),
            Place::Person(12),
            Place::Site(1),
            Place::JobInput(4),
            Place::JobInput(5),
            Place::JobOutput(9),
        ];
        let index = PlaceIndex::new(&rows, |p| *p);
        for (position, &place) in rows.iter().enumerate() {
            assert_eq!(index.locate(place), Some(position));
        }
        for missing in [
            Place::Inventory(3),
            Place::Person(9),
            Place::Person(11),
            Place::Person(13),
            Place::Site(u64::MAX),
            Place::JobInput(6),
            Place::JobOutput(8),
            Place::Stock { region: 1, good: 3 },
        ] {
            assert_eq!(index.locate(missing), None);
        }
        assert_eq!(
            PlaceIndex::<Place>::new(&[], |p| *p).locate(Place::Person(1)),
            None
        );
    }
}
