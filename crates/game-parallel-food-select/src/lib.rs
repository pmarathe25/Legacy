//! Select the kth eligible identity with bounded parallel radix passes.
use game_parallel_fused_data::Block;
use rayon::prelude::*;

pub fn cutoff(blocks: &[Block], rank: usize) -> u64 {
    let low = blocks
        .par_iter()
        .filter(|block| block.journey.is_none())
        .filter_map(|block| block.ids.first().copied())
        .min()
        .expect("claiming block");
    let high = blocks
        .par_iter()
        .filter(|block| block.journey.is_none())
        .filter_map(|block| block.ids.last().copied())
        .max()
        .expect("claiming block");
    let difference = low ^ high;
    if difference == 0 {
        return low;
    }
    let highest = (63 - difference.leading_zeros()) / 8;
    let (mut mask, mut prefix) = if highest == 7 {
        (0, 0)
    } else {
        let shift = (highest + 1) * 8;
        let mask = u64::MAX << shift;
        (mask, low & mask)
    };
    let mut rank = rank;
    for byte in (0..=highest).rev() {
        let shift = byte * 8;
        let counts = blocks
            .par_iter()
            .fold(
                || [0usize; 256],
                |mut counts, block| {
                    if block.journey.is_none() {
                        for (index, &id) in block.ids.iter().enumerate() {
                            if id & mask == prefix && block.food_claim[index] {
                                counts[((id >> shift) & 255) as usize] += 1;
                            }
                        }
                    }
                    counts
                },
            )
            .reduce(
                || [0usize; 256],
                |mut a, b| {
                    for (a, b) in a.iter_mut().zip(b) {
                        *a += b;
                    }
                    a
                },
            );
        let bucket = counts
            .iter()
            .enumerate()
            .find_map(|(i, &n)| {
                if rank < n {
                    Some(i)
                } else {
                    rank -= n;
                    None
                }
            })
            .expect("food rank is within claims");
        prefix |= (bucket as u64) << shift;
        mask |= 255u64 << shift;
    }
    prefix
}

/// Rank in a circular ordering without overflow.
pub fn rotated_rank(index: usize, start: usize, length: usize, offset: usize) -> usize {
    let relative = index - start;
    if relative >= offset {
        relative - offset
    } else {
        length - (offset - relative)
    }
}
