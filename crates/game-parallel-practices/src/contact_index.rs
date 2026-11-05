//! Read-only lookup of actually observed directed contacts.
use rayon::prelude::*;

pub struct ContactIndex<'a> {
    contacts: &'a [(u64, u64)],
    dense: Option<(u64, Vec<usize>)>,
}

impl<'a> ContactIndex<'a> {
    pub fn new(contacts: &'a [(u64, u64)], reporters: usize) -> Self {
        let dense = contacts
            .first()
            .zip(contacts.last())
            .and_then(|(first, last)| {
                let span = last.0.checked_sub(first.0)?.checked_add(1)?;
                if contacts.len() < 4096
                    || reporters < contacts.len() / 16
                    || span > contacts.len() as u64
                {
                    return None;
                }
                let groups: Vec<_> = contacts
                    .par_iter()
                    .enumerate()
                    .filter_map(|(i, row)| {
                        (i == 0 || contacts[i - 1].0 != row.0).then_some((row.0, i))
                    })
                    .collect();
                (span == groups.len() as u64)
                    .then(|| (first.0, groups.into_par_iter().map(|row| row.1).collect()))
            });
        Self { contacts, dense }
    }

    pub fn contains(&self, person: u64, other: u64) -> bool {
        if let Some((first, starts)) = &self.dense {
            let Some(index) = person
                .checked_sub(*first)
                .and_then(|i| usize::try_from(i).ok())
            else {
                return false;
            };
            let Some(&start) = starts.get(index) else {
                return false;
            };
            let end = starts
                .get(index + 1)
                .copied()
                .unwrap_or(self.contacts.len());
            self.contacts[start..end]
                .binary_search_by_key(&other, |row| row.1)
                .is_ok()
        } else {
            self.contacts.binary_search(&(person, other)).is_ok()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ContactIndex;

    #[test]
    fn dense_and_sparse_indexes_only_match_real_contacts() {
        let dense: Vec<_> = (1..=5000)
            .flat_map(|person| [(person, 7), (person, 9)])
            .collect();
        let indexed = ContactIndex::new(&dense, 5000);
        assert!(indexed.dense.is_some());
        for person in [0, 1, 2499, 5000, 5001, u64::MAX] {
            for other in [6, 7, 8, 9, 10] {
                assert_eq!(
                    indexed.contains(person, other),
                    dense.binary_search(&(person, other)).is_ok()
                );
            }
        }
        assert!(ContactIndex::new(&dense, 1).dense.is_none());
        let sparse: Vec<_> = (1..=5000)
            .flat_map(|person| [(person * 2, 7), (person * 2, 9)])
            .collect();
        let fallback = ContactIndex::new(&sparse, 5000);
        assert!(fallback.dense.is_none());
        assert!(fallback.contains(10, 9));
        assert!(!fallback.contains(9, 9));
        assert!(!ContactIndex::new(&[], 0).contains(1, 2));
    }
}
