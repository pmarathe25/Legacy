//! Rigid transmissions exchange force for travel; branching divides the available effort.
use game_core::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Effort {
    pub force: u64,
    pub travel: u64,
}
impl Effort {
    pub fn work(self) -> u128 {
        u128::from(self.force) * u128::from(self.travel)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transmission {
    /// An earlier element, or the external source. Topological ordering rules out cycles.
    pub input: Option<usize>,
    pub drive: u32,
    pub load: u32,
    pub efficiency: u16,
    pub limit: u64,
}
impl Transmission {
    pub fn validate(&self) -> Result<()> {
        require(
            (1..=1000).contains(&self.drive)
                && (1..=1000).contains(&self.load)
                && self.efficiency <= 1000
                && self.limit > 0,
            "invalid mechanical dimensions or material limits",
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resolution {
    pub outputs: Vec<Effort>,
    pub broken: Vec<usize>,
    pub delivered_work: u128,
}

pub fn resolve(parts: &[Transmission], source: Effort) -> Result<Resolution> {
    require(
        !parts.is_empty() && parts.len() <= 32,
        "invalid assembly size",
    )?;
    let mut branches = vec![0u64; parts.len()];
    let mut roots = 0u64;
    for (i, part) in parts.iter().enumerate() {
        part.validate()?;
        if let Some(parent) = part.input {
            require(parent < i, "connection must join an earlier element")?;
            branches[parent] += 1;
        } else {
            roots += 1;
        }
    }
    let mut outputs: Vec<Effort> = Vec::with_capacity(parts.len());
    let mut broken = vec![];
    let mut delivered_work = 0;
    for (i, part) in parts.iter().enumerate() {
        let (input, sharing) = part
            .input
            .map_or((source, roots), |p| (outputs[p], branches[p]));
        let input = Effort {
            force: input.force / sharing,
            travel: input.travel,
        };
        let force = u128::from(input.force) * u128::from(part.drive) * u128::from(part.efficiency)
            / u128::from(part.load)
            / 1000;
        let travel = u128::from(input.travel) * u128::from(part.load) / u128::from(part.drive);
        let output = if force > u128::from(part.limit) || input.force > part.limit {
            broken.push(i);
            Effort::default()
        } else {
            Effort {
                force: force as u64,
                travel: travel.min(u128::from(u64::MAX)) as u64,
            }
        };
        if branches[i] == 0 {
            delivered_work += output.work();
        }
        outputs.push(output);
    }
    debug_assert!(delivered_work <= source.work());
    Ok(Resolution {
        outputs,
        broken,
        delivered_work,
    })
}

/// Quasi-static motor feedback uses only the effort needed to move the specified load.
/// Capacity is private physics; observers receive the moved load and felt effort.
pub fn resolve_loaded(
    parts: &[Transmission],
    available: Effort,
    load: u64,
    outlet: usize,
) -> Result<(Effort, Resolution)> {
    require(
        outlet < parts.len() && load > 0 && !parts.iter().any(|p| p.input == Some(outlet)),
        "load needs a terminal output",
    )?;
    let unlimited: Vec<_> = parts
        .iter()
        .map(|p| Transmission {
            limit: u64::MAX,
            ..*p
        })
        .collect();
    // Overflow on the selected path means the unlimited capacity probe exceeded
    // the representable force. It still suffices for searching a lower input.
    resolve(&unlimited, available)?;
    let mut path = vec![outlet];
    while let Some(parent) = unlimited[*path.last().unwrap()].input {
        path.push(parent);
    }
    let sufficient = |force| -> Result<bool> {
        let probe = resolve(&unlimited, Effort { force, ..available })?;
        Ok(probe.outputs[outlet].force >= load || probe.broken.iter().any(|i| path.contains(i)))
    };
    let mut applied = available;
    if sufficient(available.force)? {
        let (mut low, mut high) = (1, available.force);
        while low < high {
            let mid = low + (high - low) / 2;
            if sufficient(mid)? {
                high = mid
            } else {
                low = mid + 1
            }
        }
        applied.force = low;
    }
    Ok((applied, resolve(parts, applied)?))
}
