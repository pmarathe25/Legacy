//! Personal response to directly received one-sided transfers.
use crate::{Decision, Offer, Outcome};
use rayon::prelude::*;

pub fn gifts(offers: &[Offer], decisions: &[Decision]) -> Vec<(u64, u64)> {
    let mut gifts: Vec<_> = offers
        .par_iter()
        .zip(decisions.par_iter())
        .filter_map(|(offer, decision)| {
            if decision.outcome != Outcome::Accepted {
                return None;
            }
            match (offer.give.is_empty(), offer.receive.is_empty()) {
                (false, true) => Some((offer.recipient, offer.proposer)),
                (true, false) => Some((offer.proposer, offer.recipient)),
                _ => None,
            }
        })
        .collect();
    gifts.par_sort_unstable();
    gifts
}
