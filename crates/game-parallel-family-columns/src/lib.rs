//! Stable identity-ordered lifecycle storage; physical movement never relocates these rows.
use game_parallel_family_model::{Member, Pregnancy};
use game_parallel_genetics::Genome;
use rayon::prelude::*;
pub const BLOCK: usize = 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct State {
    pub age: u64,
    pub lifespan_loss: u64,
    pub partner: Option<u64>,
    pub pregnancy: Option<(u64, u64)>,
    pub next_conception: u64,
    pub alive: bool,
    pub reproduction_allowed: bool,
}
impl From<&Member> for State {
    fn from(member: &Member) -> Self {
        Self {
            age: member.age,
            lifespan_loss: member.lifespan_loss,
            partner: member.partner,
            pregnancy: member.pregnancy.as_ref().map(|p| (p.father, p.due)),
            next_conception: member.next_conception,
            alive: member.alive,
            reproduction_allowed: member.reproduction_allowed,
        }
    }
}
#[derive(Clone)]
pub struct Metadata {
    pub id: u64,
    pub born: Option<u64>,
    pub reconsider_partnership: bool,
    pub genome: Genome,
    pub parents: [u64; 2],
    pub parent_count: usize,
    pub ancestors: [u64; 7],
    pub ancestor_count: usize,
}
impl Metadata {
    fn from_member(member: &Member) -> Self {
        let mut parents = [0; 2];
        parents[..member.parents.len()].copy_from_slice(&member.parents);
        Self {
            id: member.id,
            born: member.born,
            reconsider_partnership: member.reconsider_partnership,
            genome: member.genome.clone(),
            parents,
            parent_count: member.parents.len(),
            ancestors: [0; 7],
            ancestor_count: 0,
        }
    }
    pub fn related(&self, other: &Self) -> bool {
        self.ancestors[..self.ancestor_count]
            .iter()
            .any(|id| other.ancestors[..other.ancestor_count].contains(id))
    }
    pub fn member(&self, state: State) -> Member {
        Member {
            id: self.id,
            born: self.born,
            genome: self.genome.clone(),
            parents: self.parents[..self.parent_count].iter().copied().collect(),
            age: state.age,
            lifespan_loss: state.lifespan_loss,
            partner: state.partner,
            pregnancy: state
                .pregnancy
                .map(|(father, due)| Pregnancy { father, due }),
            next_conception: state.next_conception,
            alive: state.alive,
            reproduction_allowed: state.reproduction_allowed,
            reconsider_partnership: self.reconsider_partnership,
        }
    }
}
#[derive(Default)]
pub struct Columns {
    pub age: Vec<u64>,
    pub lifespan_loss: Vec<u64>,
    pub partner: Vec<Option<u64>>,
    pub pregnancy: Vec<Option<(u64, u64)>>,
    pub next_conception: Vec<u64>,
    pub alive: Vec<bool>,
    pub reproduction_allowed: Vec<bool>,
}
impl Columns {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            age: Vec::with_capacity(capacity),
            lifespan_loss: Vec::with_capacity(capacity),
            partner: Vec::with_capacity(capacity),
            pregnancy: Vec::with_capacity(capacity),
            next_conception: Vec::with_capacity(capacity),
            alive: Vec::with_capacity(capacity),
            reproduction_allowed: Vec::with_capacity(capacity),
        }
    }
    pub fn read(&self, i: usize) -> State {
        State {
            age: self.age[i],
            lifespan_loss: self.lifespan_loss[i],
            partner: self.partner[i],
            pregnancy: self.pregnancy[i],
            next_conception: self.next_conception[i],
            alive: self.alive[i],
            reproduction_allowed: self.reproduction_allowed[i],
        }
    }
    pub fn write(&mut self, i: usize, state: State) {
        self.age[i] = state.age;
        self.lifespan_loss[i] = state.lifespan_loss;
        self.partner[i] = state.partner;
        self.pregnancy[i] = state.pregnancy;
        self.next_conception[i] = state.next_conception;
        self.alive[i] = state.alive;
        self.reproduction_allowed[i] = state.reproduction_allowed;
    }
    pub fn push(&mut self, state: State) {
        self.age.push(state.age);
        self.lifespan_loss.push(state.lifespan_loss);
        self.partner.push(state.partner);
        self.pregnancy.push(state.pregnancy);
        self.next_conception.push(state.next_conception);
        self.alive.push(state.alive);
        self.reproduction_allowed.push(state.reproduction_allowed);
    }
}
#[derive(Default)]
pub struct Block {
    pub current: Columns,
    pub next: Columns,
}
pub struct Store {
    pub metadata: Vec<Metadata>,
    pub blocks: Vec<Block>,
    original_order: Option<Vec<u64>>,
}
pub fn locate(metadata: &[Metadata], id: u64) -> Option<usize> {
    let first = metadata.first()?.id;
    if metadata.last()?.id.checked_sub(first) == Some((metadata.len() - 1) as u64) {
        usize::try_from(id.checked_sub(first)?)
            .ok()
            .filter(|&i| i < metadata.len())
    } else {
        metadata.binary_search_by_key(&id, |row| row.id).ok()
    }
}
fn ancestors(metadata: &[Metadata], row: &Metadata) -> Result<([u64; 7], usize), &'static str> {
    let mut ids = [0; 7];
    ids[0] = row.id;
    let mut count = 1;
    for &parent in &row.parents[..row.parent_count] {
        ids[count] = parent;
        count += 1;
        let parent = &metadata[locate(metadata, parent).ok_or("missing family parent")?];
        for &grandparent in &parent.parents[..parent.parent_count] {
            ids[count] = grandparent;
            count += 1;
        }
    }
    Ok((ids, count))
}
impl Store {
    pub fn compile(members: &[Member]) -> Result<Self, &'static str> {
        if members
            .iter()
            .any(|member| member.id == 0 || ![0, 2].contains(&member.parents.len()))
        {
            return Err("invalid family metadata");
        }
        let mut members: Vec<_> = members.iter().collect();
        let original_order = Some(members.iter().map(|member| member.id).collect());
        members.par_sort_unstable_by_key(|member| member.id);
        if members.par_windows(2).any(|pair| pair[0].id == pair[1].id) {
            return Err("duplicate family identity");
        }
        let mut metadata: Vec<_> = members
            .par_iter()
            .map(|member| Metadata::from_member(member))
            .collect();
        let ancestry: Vec<_> = metadata
            .par_iter()
            .map(|row| ancestors(&metadata, row))
            .collect::<Result<_, _>>()?;
        metadata
            .par_iter_mut()
            .zip(ancestry)
            .for_each(|(row, (ids, count))| {
                row.ancestors = ids;
                row.ancestor_count = count;
            });
        let blocks = members
            .par_chunks(BLOCK)
            .map(|chunk| {
                let mut block = Block::default();
                for member in chunk {
                    let state = State::from(*member);
                    block.current.push(state);
                    block.next.push(state);
                }
                block
            })
            .collect();
        Ok(Self {
            metadata,
            blocks,
            original_order,
        })
    }
    pub fn locate(&self, id: u64) -> Option<usize> {
        locate(&self.metadata, id)
    }
    pub fn state(&self, slot: usize, staged: bool) -> State {
        let block = &self.blocks[slot / BLOCK];
        if staged {
            block.next.read(slot % BLOCK)
        } else {
            block.current.read(slot % BLOCK)
        }
    }
    pub fn export(&self) -> Vec<Member> {
        if let Some(order) = &self.original_order {
            order
                .par_iter()
                .map(|&id| {
                    let i = self.locate(id).unwrap();
                    self.metadata[i].member(self.state(i, false))
                })
                .collect()
        } else {
            self.metadata
                .par_iter()
                .enumerate()
                .map(|(i, row)| row.member(self.state(i, false)))
                .collect()
        }
    }
    /// Called only after the complete lifecycle draft and every other owner succeed.
    pub fn publish(&mut self, newborns: Vec<Member>) {
        self.blocks
            .par_iter_mut()
            .for_each(|block| std::mem::swap(&mut block.current, &mut block.next));
        self.original_order = None;
        if newborns.is_empty() {
            return;
        }
        // All parents are committed identities; same-phase births cannot be parents.
        let (metadata, states): (Vec<_>, Vec<_>) = newborns
            .into_par_iter()
            .map(|child| {
                let mut row = Metadata::from_member(&child);
                let (ids, count) =
                    ancestors(&self.metadata, &row).expect("validated newborn pedigree");
                row.ancestors = ids;
                row.ancestor_count = count;
                (row, State::from(&child))
            })
            .unzip();
        debug_assert!(
            self.metadata
                .last()
                .is_none_or(|last| last.id < metadata[0].id)
        );
        debug_assert!(metadata.windows(2).all(|pair| pair[0].id < pair[1].id));
        let partial = self.metadata.len() % BLOCK;
        let tail = if partial == 0 {
            0
        } else {
            (BLOCK - partial).min(states.len())
        };
        if tail != 0 {
            let block = self.blocks.last_mut().unwrap();
            for &state in &states[..tail] {
                block.current.push(state);
                block.next.push(state);
            }
        }
        let blocks: Vec<_> = states[tail..]
            .par_chunks(BLOCK)
            .map(|chunk| {
                let mut block = Block {
                    current: Columns::with_capacity(BLOCK),
                    next: Columns::with_capacity(BLOCK),
                };
                for &state in chunk {
                    block.current.push(state);
                    block.next.push(state);
                }
                block
            })
            .collect();
        self.metadata.par_extend(metadata);
        self.blocks.extend(blocks);
    }
}
