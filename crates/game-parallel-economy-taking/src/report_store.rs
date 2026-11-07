//! Immutable recipient-keyed pages. Flat serialization is independent of layout.
use crate::{Outcome, Report, ReportBelief};
use rayon::prelude::*;
use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::SerializeSeq};
use std::{cmp::Reverse, sync::Arc};

// A page covers 32 recipient IDs. With at most 64 reports per recipient,
// a validated page contains at most 2,048 packed reports.
const PAGE_BITS: u32 = 5;
const DEPTH: usize = 8;
const MAX_REPORTS: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Node {
    Branch(Vec<(u8, Arc<Node>)>),
    Leaf(Vec<Report>),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReportStore {
    root: Option<Arc<Node>>,
    len: usize,
}

fn digit(recipient: u64, depth: usize) -> u8 {
    let page = recipient >> PAGE_BITS;
    ((page >> (8 * (DEPTH - depth - 1))) & 0xff) as u8
}
fn bucket(rows: &[Report], recipient: u64) -> &[Report] {
    let begin = rows.partition_point(|r| r.recipient < recipient);
    let end = rows.partition_point(|r| r.recipient <= recipient);
    &rows[begin..end]
}

pub(crate) struct DenseLookup<'a> {
    first_page: u64,
    pages: Vec<&'a [Report]>,
}
impl<'a> DenseLookup<'a> {
    pub fn get(&self, recipient: u64) -> &'a [Report] {
        let Some(index) = (recipient >> PAGE_BITS)
            .checked_sub(self.first_page)
            .and_then(|n| usize::try_from(n).ok())
        else {
            return &[];
        };
        self.pages
            .get(index)
            .map_or(&[], |rows| bucket(rows, recipient))
    }
}

fn identity(report: &Report) -> (u64, u64, u64) {
    (report.recipient, report.speaker, report.actor)
}
fn outcome_rank(outcome: Outcome) -> u8 {
    match outcome {
        Outcome::Transferred => 0,
        Outcome::Unavailable => 1,
        Outcome::Resisted => 2,
        Outcome::Conflict => 3,
    }
}
fn belief_rank(belief: ReportBelief) -> u8 {
    match belief {
        ReportBelief::Unassessed => 0,
        ReportBelief::Believes => 1,
        ReportBelief::Doubts => 2,
    }
}
type WinnerKey = (
    (u64, u64, u64),
    Reverse<u64>,
    (u64, u32, u64, u64, u8, Option<u64>, u8),
);
fn winner_key(report: &Report) -> WinnerKey {
    (
        identity(report),
        Reverse(report.received_at),
        (
            report.target,
            report.good,
            report.quantity,
            report.observed_at,
            outcome_rank(report.claimed_outcome),
            report.source_speaker,
            belief_rank(report.belief),
        ),
    )
}

/// Update one recipient without sorting or regrouping unrelated page contents.
fn update_one(old: &[Report], incoming: &Report) -> Option<Vec<Report>> {
    let begin = old.partition_point(|r| r.recipient < incoming.recipient);
    let end = old.partition_point(|r| r.recipient <= incoming.recipient);
    let mut recipient = old[begin..end].to_vec();
    let key = (incoming.speaker, incoming.actor);
    match recipient.binary_search_by_key(&key, |r| (r.speaker, r.actor)) {
        Ok(index) => {
            if recipient[index].received_at >= incoming.received_at {
                return None;
            }
            recipient[index] = incoming.clone();
        }
        Err(_) => {
            if recipient.len() == MAX_REPORTS {
                let (worst, row) = recipient
                    .iter()
                    .enumerate()
                    .max_by_key(|(_, r)| (Reverse(r.received_at), r.speaker, r.actor))?;
                if (
                    Reverse(incoming.received_at),
                    incoming.speaker,
                    incoming.actor,
                ) >= (Reverse(row.received_at), row.speaker, row.actor)
                {
                    return None;
                }
                recipient.remove(worst);
            }
            let index = recipient
                .binary_search_by_key(&key, |r| (r.speaker, r.actor))
                .unwrap_err();
            recipient.insert(index, incoming.clone());
        }
    }
    let mut rows = Vec::with_capacity(begin + recipient.len() + old.len() - end);
    rows.extend_from_slice(&old[..begin]);
    rows.extend(recipient);
    rows.extend_from_slice(&old[end..]);
    Some(rows)
}

/// Update a page after the global batch has been deduplicated and bounded.
fn update_leaf(old: &[Report], incoming: &[Report]) -> (Arc<Node>, i64) {
    // New pages already contain canonical, deduplicated, bounded recipients.
    if old.is_empty() {
        return (
            Arc::new(Node::Leaf(incoming.to_vec())),
            incoming.len() as i64,
        );
    }
    let mut rows: Vec<_> = old
        .iter()
        .cloned()
        .map(|r| (r, false))
        .chain(incoming.iter().cloned().map(|r| (r, true)))
        .collect();
    // At equal time an existing entry wins; valid simulation boundaries always
    // advance, but this also gives malformed callers a deterministic result.
    rows.sort_unstable_by_key(|(r, new)| (identity(r), Reverse(r.received_at), *new));
    let mut latest: Vec<_> = rows
        .chunk_by(|a, b| identity(&a.0) == identity(&b.0))
        .map(|group| group[0].0.clone())
        .collect();
    latest.sort_unstable_by_key(|r| (r.recipient, Reverse(r.received_at), r.speaker, r.actor));
    let mut retained: Vec<_> = latest
        .chunk_by(|a, b| a.recipient == b.recipient)
        .flat_map(|group| group.iter().take(MAX_REPORTS).cloned())
        .collect();
    retained.sort_unstable_by_key(identity);
    let delta = retained.len() as i64 - old.len() as i64;
    (Arc::new(Node::Leaf(retained)), delta)
}

/// Rebuild each dirty radix path once; untouched children remain shared.
fn update_node(old: Option<&Arc<Node>>, rows: &[Report], depth: usize) -> (Arc<Node>, i64) {
    if depth == DEPTH {
        let old_rows = match old.map(Arc::as_ref) {
            Some(Node::Leaf(rows)) => rows.as_slice(),
            _ => &[],
        };
        if let [incoming] = rows
            && let Some(old_node) = old
        {
            return match update_one(old_rows, incoming) {
                Some(updated) => {
                    let delta = updated.len() as i64 - old_rows.len() as i64;
                    (Arc::new(Node::Leaf(updated)), delta)
                }
                None => (Arc::clone(old_node), 0),
            };
        }
        return update_leaf(old_rows, rows);
    }
    let old_children = match old.map(Arc::as_ref) {
        Some(Node::Branch(children)) => children.as_slice(),
        _ => &[],
    };
    let update = |group: &[Report]| {
        let key = digit(group[0].recipient, depth);
        let prior = old_children
            .binary_search_by_key(&key, |row| row.0)
            .ok()
            .map(|i| &old_children[i].1);
        let (child, delta) = update_node(prior, group, depth + 1);
        (key, child, delta)
    };
    let updates: Vec<_> = if rows.len() >= 2048 {
        rows.par_chunk_by(|a, b| digit(a.recipient, depth) == digit(b.recipient, depth))
            .map(update)
            .collect()
    } else {
        rows.chunk_by(|a, b| digit(a.recipient, depth) == digit(b.recipient, depth))
            .map(update)
            .collect()
    };
    let mut children = old_children.to_vec();
    let mut delta = 0;
    let mut changed = false;
    for (key, child, change) in updates {
        match children.binary_search_by_key(&key, |row| row.0) {
            Ok(index) => {
                changed |= !Arc::ptr_eq(&children[index].1, &child);
                children[index].1 = child;
            }
            Err(index) => {
                changed = true;
                children.insert(index, (key, child));
            }
        }
        delta += change;
    }
    if !changed && let Some(old) = old {
        return (Arc::clone(old), 0);
    }
    (Arc::new(Node::Branch(children)), delta)
}

fn build(rows: &[Report], depth: usize) -> Option<Arc<Node>> {
    if rows.is_empty() {
        return None;
    }
    if depth == DEPTH {
        return Some(Arc::new(Node::Leaf(rows.to_vec())));
    }
    let children = rows
        .chunk_by(|a, b| digit(a.recipient, depth) == digit(b.recipient, depth))
        .map(|group| {
            (
                digit(group[0].recipient, depth),
                build(group, depth + 1).expect("nonempty page"),
            )
        })
        .collect();
    Some(Arc::new(Node::Branch(children)))
}

impl From<Vec<Report>> for ReportStore {
    fn from(mut rows: Vec<Report>) -> Self {
        rows.par_sort_unstable_by_key(|r| (r.recipient, r.speaker, r.actor));
        Self {
            root: build(&rows, 0),
            len: rows.len(),
        }
    }
}

impl ReportStore {
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    /// All retained reports belonging to one recipient, in canonical key order.
    pub fn get(&self, recipient: u64) -> &[Report] {
        let mut node = self.root.as_deref();
        for depth in 0..DEPTH {
            node = match node {
                Some(Node::Branch(children)) => children
                    .binary_search_by_key(&digit(recipient, depth), |c| c.0)
                    .ok()
                    .map(|index| children[index].1.as_ref()),
                _ => return &[],
            };
        }
        let Some(Node::Leaf(rows)) = node else {
            return &[];
        };
        bucket(rows, recipient)
    }
    pub fn iter(&self) -> Iter<'_> {
        Iter {
            pending: self.root.iter().map(Arc::as_ref).collect(),
            current: None,
        }
    }
    pub fn first(&self) -> Option<&Report> {
        self.iter().next()
    }
    pub fn last(&self) -> Option<&Report> {
        let mut node = self.root.as_deref()?;
        loop {
            match node {
                Node::Branch(children) => node = children.last()?.1.as_ref(),
                Node::Leaf(rows) => return rows.last(),
            }
        }
    }
    /// Build a transient direct page index only when occupied pages are contiguous.
    pub(crate) fn dense_lookup(&self) -> Option<DenseLookup<'_>> {
        let first_page = self.first()?.recipient >> PAGE_BITS;
        let last_page = self.last()?.recipient >> PAGE_BITS;
        let span = last_page.checked_sub(first_page)?.checked_add(1)?;
        if span > self.len as u64 {
            return None;
        }
        let mut stack = vec![self.root.as_deref()?];
        let mut pages = Vec::new();
        while let Some(node) = stack.pop() {
            match node {
                Node::Branch(children) => {
                    stack.extend(children.iter().rev().map(|row| row.1.as_ref()))
                }
                Node::Leaf(rows) => pages.push(rows.as_slice()),
            }
        }
        (usize::try_from(span).ok() == Some(pages.len()))
            .then_some(DenseLookup { first_page, pages })
    }

    /// Return a new generation and the bounded incoming candidates. No existing
    /// node is changed, so a discarded draft leaves the old generation intact.
    pub fn updated(&self, mut incoming: Vec<Report>) -> (Self, Vec<Report>) {
        if incoming.is_empty() {
            return (self.clone(), incoming);
        }
        #[cfg(feature = "profile")]
        let mut phase = game_diagnostics::span("taking.report_store.normalize");
        let bounded: Vec<_> = if incoming
            .par_windows(2)
            .all(|p| p[0].recipient < p[1].recipient)
        {
            // One already-ordered report per recipient needs no regrouping.
            incoming
        } else if incoming.par_windows(2).all(|p| {
            identity(&p[0]) < identity(&p[1])
                && (p[0].recipient != p[1].recipient || p[0].received_at == p[1].received_at)
        }) {
            // Distinct keys with one timestamp per recipient are already in
            // retention order, so a hot recipient needs only its first 64.
            incoming
                .par_chunk_by(|a, b| a.recipient == b.recipient)
                .flat_map_iter(|group| group.iter().take(MAX_REPORTS).cloned())
                .collect()
        } else {
            incoming.par_sort_unstable_by_key(winner_key);
            let mut distinct: Vec<_> = incoming
                .par_chunk_by(|a, b| identity(a) == identity(b))
                .map(|group| group[0].clone())
                .collect();
            distinct.par_sort_unstable_by_key(|r| {
                (r.recipient, Reverse(r.received_at), r.speaker, r.actor)
            });
            // Any incoming record after the first 64 is dominated by 64
            // different identities, even if older entries win.
            let mut bounded: Vec<_> = distinct
                .par_chunk_by(|a, b| a.recipient == b.recipient)
                .flat_map_iter(|group| group.iter().take(MAX_REPORTS).cloned())
                .collect();
            bounded.par_sort_unstable_by_key(identity);
            bounded
        };
        #[cfg(feature = "profile")]
        {
            drop(phase);
            phase = game_diagnostics::span("taking.report_store.pages");
        }
        let (root, delta) = update_node(self.root.as_ref(), &bounded, 0);
        #[cfg(feature = "profile")]
        drop(phase);
        let len =
            usize::try_from(self.len as i128 + i128::from(delta)).expect("validated report count");
        (
            Self {
                root: Some(root),
                len,
            },
            bounded,
        )
    }
}

pub struct Iter<'a> {
    pending: Vec<&'a Node>,
    current: Option<std::slice::Iter<'a, Report>>,
}
impl<'a> Iterator for Iter<'a> {
    type Item = &'a Report;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(report) = self.current.as_mut().and_then(Iterator::next) {
                return Some(report);
            }
            match self.pending.pop()? {
                Node::Branch(children) => self
                    .pending
                    .extend(children.iter().rev().map(|(_, node)| node.as_ref())),
                Node::Leaf(rows) => self.current = Some(rows.iter()),
            }
        }
    }
}
impl Serialize for ReportStore {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(self.len))?;
        for report in self.iter() {
            seq.serialize_element(report)?;
        }
        seq.end()
    }
}
impl<'de> Deserialize<'de> for ReportStore {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Vec::<Report>::deserialize(deserializer).map(Self::from)
    }
}

#[cfg(test)]
#[path = "../tests/support/report_store.rs"]
mod tests;
