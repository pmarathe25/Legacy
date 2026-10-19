//! Pure inherited trait calculations; interpretation belongs to consumers.
mod gene_name;
mod shared;
use game_core::*;
pub use gene_name::GeneName;
use serde::{Deserialize, Serialize};
pub use shared::{Genome, deserialize_genome};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Alleles(pub [u16; 2]);
pub fn mutate(seed: u64, identity: u64, parent: Alleles, mutation: u16) -> Alleles {
    let width = u64::from(mutation) * 2 + 1;
    Alleles(std::array::from_fn(|i| {
        let delta = (random(seed, identity, i as u64 + 10) % width) as i64 - i64::from(mutation);
        (i64::from(parent.0[i]) + delta).clamp(0, 65535) as u16
    }))
}
/// Preserve the expressed starting value while varying the two inherited copies.
pub fn founder(seed: u64, person: PersonId, value: u16, spread: u16) -> Alleles {
    let width = value.min(u16::MAX - value).min(spread);
    let delta = (random(seed, person.0, 7) % (u64::from(width) + 1)) as u16;
    let pair = [value - delta, value + delta];
    Alleles(if random(seed, person.0, 8) & 1 == 0 {
        pair
    } else {
        [pair[1], pair[0]]
    })
}
pub fn inherit_bounded(
    seed: u64,
    child: PersonId,
    mother: Alleles,
    father: Alleles,
    mutation: u16,
    maximum: u16,
) -> Alleles {
    let mut result = inherit(seed, child, mother, father, mutation);
    result.0.iter_mut().for_each(|v| *v = (*v).min(maximum));
    result
}
pub fn inherit(
    seed: u64,
    child: PersonId,
    mother: Alleles,
    father: Alleles,
    mutation: u16,
) -> Alleles {
    let m = mother.0[(random(seed, child.0, 0) & 1) as usize];
    let f = father.0[(random(seed, child.0, 1) & 1) as usize];
    let change = |value: u16, counter: u64| {
        let width = u64::from(mutation) * 2 + 1;
        let delta = (random(seed, child.0, counter) % width) as i64 - i64::from(mutation);
        (i64::from(value) + delta).clamp(0, i64::from(u16::MAX)) as u16
    };
    Alleles([change(m, 2), change(f, 3)])
}
pub fn expression(alleles: Alleles) -> u16 {
    ((u32::from(alleles.0[0]) + u32::from(alleles.0[1])) / 2) as u16
}

/// Independent named founding loci do not change when unrelated loci are added.
pub fn founder_genome<'a>(
    seed: u64,
    identity: u64,
    loci: impl IntoIterator<Item = &'a str>,
    minimum: u16,
    maximum: u16,
) -> game_core::Result<std::collections::BTreeMap<String, Alleles>> {
    game_core::require(minimum <= maximum, "invalid founding allele range")?;
    let span = u64::from(maximum) - u64::from(minimum) + 1;
    Ok(loci
        .into_iter()
        .map(|key| {
            let stream = key
                .bytes()
                .fold(0u64, |h, b| h.wrapping_mul(31).wrapping_add(u64::from(b)));
            (
                key.to_owned(),
                Alleles([
                    (u64::from(minimum) + game_core::random(seed, identity, stream) % span) as u16,
                    (u64::from(minimum)
                        + game_core::random(seed, identity, stream.wrapping_add(1)) % span)
                        as u16,
                ]),
            )
        })
        .collect())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn founder_diversity_preserves_expression_and_exposes_inherited_variation() {
        for value in [0, 1, 1000, 32767, 65534, 65535] {
            for id in 1..100 {
                let a = founder(1, PersonId(id), value, 65535);
                assert_eq!(expression(a), value);
                assert_eq!(a, founder(1, PersonId(id), value, 65535));
                assert_eq!(founder(1, PersonId(id), value, 0), Alleles([value, value]));
            }
        }
        let diverse = founder(1, PersonId(1), 32767, 65535);
        assert_ne!(diverse.0[0], diverse.0[1]);
        let expressions: std::collections::BTreeSet<_> = (1..100)
            .map(|p| expression(inherit(2, PersonId(p), diverse, diverse, 0)))
            .collect();
        assert!(expressions.len() > 1);
    }
    #[test]
    fn inheritance_uses_both_parents_and_is_repeatable() {
        let m = Alleles([100, 200]);
        let f = Alleles([800, 900]);
        let a = inherit(4, PersonId(2), m, f, 0);
        assert!(m.0.contains(&a.0[0]));
        assert!(f.0.contains(&a.0[1]));
        assert_eq!(a, inherit(4, PersonId(2), m, f, 0));
        assert!(expression(a) <= 1000);
    }
}
