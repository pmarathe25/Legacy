//! Each animal scores a small sample of potential mates by its own expressed taste.
use game_genetics::{Genome, random};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MateChoice {
    pub trait_locus: usize,
    pub preference_locus: usize,
    pub minimum_fit: u16,
    pub sample_size: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flexibility: Option<MateFlexibility>,
}

/// Expression above neutral makes the animal less selective. Below neutral
/// it becomes more selective. This changes neither fertility nor mate desire.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MateFlexibility {
    pub locus: usize,
    pub weight: u16,
}

fn mean(alleles: [u16; 2]) -> u32 {
    (u32::from(alleles[0]) + u32::from(alleles[1])) / 2
}

impl MateChoice {
    pub(super) fn valid(self) -> bool {
        self.trait_locus < 4
            && self.preference_locus < 4
            && self.minimum_fit <= 1000
            && (1..=8).contains(&self.sample_size)
            && self
                .flexibility
                .is_none_or(|flexibility| flexibility.locus < 4 && flexibility.weight <= 1000)
    }

    fn fit(self, observer: &Genome, other: &Genome) -> Option<u16> {
        let preferred = mean(*observer.alleles.get(self.preference_locus)?);
        let seen = mean(*other.alleles.get(self.trait_locus)?);
        Some(1000u32.saturating_sub(preferred.abs_diff(seen)) as u16)
    }

    fn minimum_for(self, observer: &Genome) -> u16 {
        let adjustment = self
            .flexibility
            .and_then(|row| {
                observer
                    .alleles
                    .get(row.locus)
                    .map(|&alleles| (row, alleles))
            })
            .map_or(0, |(row, alleles)| {
                (mean(alleles) as i32 - 500) * i32::from(row.weight) / 500
            });
        (i32::from(self.minimum_fit) - adjustment).clamp(0, 1000) as u16
    }
}

pub(super) fn select<'a, Candidate>(
    female: &super::Animal,
    candidates: &'a [Candidate],
    body: impl Fn(&Candidate) -> (&Genome, u64),
    preference: Option<MateChoice>,
    seed: u64,
    tick: u64,
) -> Option<&'a Candidate> {
    if candidates.is_empty() {
        return None;
    }
    let first = (random(seed ^ 0x6d617465, female.id, tick) % candidates.len() as u64) as usize;
    let Some(preference) = preference else {
        return candidates.get(first);
    };
    let minimum_attraction = preference.minimum_for(&female.genome);
    candidates[first..]
        .iter()
        .chain(&candidates[..first])
        .take(preference.sample_size)
        .filter_map(|candidate| {
            let (genome, male) = body(candidate);
            let attraction = preference.fit(&female.genome, genome)?;
            let response = preference.fit(genome, &female.genome)?;
            (attraction >= minimum_attraction && response >= preference.minimum_for(genome))
                .then_some((
                    attraction,
                    random(seed ^ female.id, male, tick),
                    male,
                    candidate,
                ))
        })
        .max_by_key(|&(fit, tie, male, _)| (fit, tie, male))
        .map(|(_, _, _, candidate)| candidate)
}
