//! Compact immutable job metadata retained in fixed-size segments.
use game_parallel_packed_jobs_model::{Block, Bundle, Row};
use game_parallel_production_model::{Amount, Job, JobPhase};
use rayon::prelude::*;
use std::collections::HashMap;
const BLOCK: usize = 4096;
type Key<'a> = (&'a [Amount], &'a [Amount]);
pub struct Store {
    chunks: Vec<Vec<Row>>,
    pub bundles: Vec<Bundle>,
    len: usize,
}
impl Store {
    pub fn empty() -> Self {
        Self {
            chunks: Vec::new(),
            bundles: Vec::new(),
            len: 0,
        }
    }
    pub fn new(mut jobs: Vec<Job>) -> Result<Self, String> {
        if jobs.par_windows(2).any(|pair| pair[0].id > pair[1].id) {
            jobs.par_sort_unstable_by_key(|job| job.id);
        }
        let blocks: Vec<_> = jobs
            .par_chunks(BLOCK)
            .map(|chunk| {
                let mut bundles = Vec::<Bundle>::new();
                let mut index: HashMap<Key<'_>, u32> = HashMap::new();
                let mut rows = Vec::with_capacity(chunk.len());
                for job in chunk {
                    let key = (job.reserved.as_slice(), job.outputs.as_slice());
                    let bundle = *index.entry(key).or_insert_with(|| {
                        let id = bundles.len() as u32;
                        bundles.push(Bundle {
                            reserved: job.reserved.clone(),
                            outputs: job.outputs.clone(),
                        });
                        id
                    });
                    rows.push(Row {
                        id: job.id,
                        worker: job.worker,
                        region: job.region,
                        recipe: job.recipe,
                        remaining: job.remaining,
                        bundle,
                        waiting: job.phase == JobPhase::Waiting,
                    });
                }
                Block { rows, bundles }
            })
            .collect();
        let mut store = Self::empty();
        store.append_blocks(blocks)?;
        Ok(store)
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn row(&self, handle: usize) -> &Row {
        &self.chunks[handle / BLOCK][handle % BLOCK]
    }
    pub fn row_mut(&mut self, handle: usize) -> &mut Row {
        &mut self.chunks[handle / BLOCK][handle % BLOCK]
    }
    pub fn find(&self, id: u64) -> Option<usize> {
        let mut low = 0usize;
        let mut high = self.len;
        while low < high {
            let middle = low + (high - low) / 2;
            if self.row(middle).id < id {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        (low < self.len && self.row(low).id == id).then_some(low)
    }
    pub fn last(&self) -> Option<&Row> {
        self.chunks.last().and_then(|chunk| chunk.last())
    }
    pub fn iter(&self) -> impl Iterator<Item = &Row> {
        self.chunks.iter().flat_map(|chunk| chunk.iter())
    }
    pub fn chunks(&self) -> &[Vec<Row>] {
        &self.chunks
    }
    /// Check every fallible append bound before a transaction publishes any owner.
    pub fn validate_append(&self, blocks: &[Block]) -> Result<(), String> {
        if blocks
            .iter()
            .take(blocks.len().saturating_sub(1))
            .any(|block| block.rows.len() != BLOCK)
            || blocks.last().is_some_and(|block| block.rows.len() > BLOCK)
        {
            return Err("invalid metadata block layout".into());
        }
        let added = blocks.iter().try_fold(0usize, |total, block| {
            total
                .checked_add(block.rows.len())
                .ok_or("job count overflow")
        })?;
        self.len.checked_add(added).ok_or("job count overflow")?;
        let bundle_total = blocks.iter().try_fold(self.bundles.len(), |total, block| {
            total
                .checked_add(block.bundles.len())
                .ok_or("too many material bundles")
        })?;
        if bundle_total > u32::MAX as usize {
            return Err("too many material bundles".into());
        }
        Ok(())
    }
    /// Append prevalidated ID-ordered blocks without flattening a full cohort.
    pub fn append_blocks(&mut self, mut blocks: Vec<Block>) -> Result<(), String> {
        if blocks
            .iter()
            .take(blocks.len().saturating_sub(1))
            .any(|block| block.rows.len() != BLOCK)
            || blocks.last().is_some_and(|block| block.rows.len() > BLOCK)
        {
            return Err("invalid metadata block layout".into());
        }
        let added = blocks.iter().try_fold(0usize, |total, block| {
            total
                .checked_add(block.rows.len())
                .ok_or("job count overflow")
        })?;
        let new_len = self.len.checked_add(added).ok_or("job count overflow")?;
        let mut bundle_total = self.bundles.len();
        let mut offsets = Vec::with_capacity(blocks.len());
        for block in &blocks {
            offsets.push(u32::try_from(bundle_total).map_err(|_| "too many material bundles")?);
            bundle_total = bundle_total
                .checked_add(block.bundles.len())
                .ok_or("too many material bundles")?;
        }
        if bundle_total > u32::MAX as usize {
            return Err("too many material bundles".into());
        }
        blocks
            .par_iter_mut()
            .zip(offsets.par_iter())
            .for_each(|(block, &offset)| {
                for row in &mut block.rows {
                    row.bundle += offset;
                }
            });
        let partial = self.len % BLOCK;
        let (fill, new_chunks) = if partial == 0 {
            (
                Vec::new(),
                blocks
                    .iter_mut()
                    .map(|block| std::mem::take(&mut block.rows))
                    .collect::<Vec<_>>(),
            )
        } else {
            let fill_len = (BLOCK - partial).min(added);
            let copy_range = |start: usize, end: usize| {
                let mut result = Vec::with_capacity(end - start);
                let mut at = start;
                while at < end {
                    let index = at / BLOCK;
                    let offset = at % BLOCK;
                    let take = (end - at).min(blocks[index].rows.len() - offset);
                    result.extend_from_slice(&blocks[index].rows[offset..offset + take]);
                    at += take;
                }
                result
            };
            let fill = copy_range(0, fill_len);
            let remaining = added - fill_len;
            let count = remaining.div_ceil(BLOCK);
            let chunks: Vec<_> = (0..count)
                .into_par_iter()
                .map(|index| {
                    let start = fill_len + index * BLOCK;
                    copy_range(start, (start + BLOCK).min(added))
                })
                .collect();
            (fill, chunks)
        };
        let bundles: Vec<_> = blocks
            .into_par_iter()
            .flat_map_iter(|block| block.bundles)
            .collect();
        if !fill.is_empty() {
            self.chunks
                .last_mut()
                .expect("partial segment")
                .extend(fill);
        }
        self.chunks
            .extend(new_chunks.into_iter().filter(|chunk| !chunk.is_empty()));
        self.bundles.extend(bundles);
        self.len = new_len;
        Ok(())
    }
    pub fn job(&self, handle: usize, remaining: u64, waiting: bool) -> Job {
        let row = self.row(handle);
        let bundle = &self.bundles[row.bundle as usize];
        Job {
            id: row.id,
            worker: row.worker,
            region: row.region,
            recipe: row.recipe,
            remaining,
            phase: if waiting {
                JobPhase::Waiting
            } else {
                JobPhase::Working
            },
            reserved: if waiting {
                Default::default()
            } else {
                bundle.reserved.clone()
            },
            outputs: bundle.outputs.clone(),
        }
    }
}
