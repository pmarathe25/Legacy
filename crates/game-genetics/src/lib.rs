//! Pure inherited trait calculations; interpretation belongs to consumers.
use game_core::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Alleles(pub [u16; 2]);
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
        (i64::from(value) + delta).clamp(0, 1000) as u16
    };
    Alleles([change(m, 2), change(f, 3)])
}
pub fn expression(alleles: Alleles) -> u16 {
    ((u32::from(alleles.0[0]) + u32::from(alleles.0[1])) / 2) as u16
}
#[cfg(test)]
mod tests {
    use super::*;
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
