use super::*;
pub type Fact<V> = game_facts::Fact<V>;
/// Payload owners preserve their schema and restrict which details reconstruction may change.
pub trait Content: game_facts::Interpretation {
    fn reconstruct(&self, draw: u64) -> Option<Self>;
    fn cues(&self, key: &str) -> Vec<String>;
}
pub type Facts<V> = game_facts::Interpreted<V>;
/// The same fact payload is used by perception, decisions, reports and creative recollection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Memories<V: Content>(
    pub game_packed::Packed<PersonId, Facts<V>>,
    pub Table<PersonId, Book>,
);
impl<V: Content> Default for Memories<V> {
    fn default() -> Self {
        Self(Default::default(), Default::default())
    }
}
type MemoryMark = (game_packed::Mark, Table<PersonId, Book>);
impl<V: Content> Resource for Memories<V> {
    // Preserve the evidence key and on-disk archive; ownership belongs to memory.
    const KEY: &'static str = "knowledge.facts";
    fn validate(&self) -> Result<()> {
        require(
            self.0.iter().all(|(p, f)| {
                f.len() == self.1.get(p).map_or(0, |b| b.entries.len()) && f.valid_confidence(None)
            }) && self.1.iter().all(|(p, b)| self.valid_book(p, b, None)),
            "invalid personal memory",
        )
    }
    fn checkpoint(&mut self) -> Checkpoint {
        Box::new((self.0.begin(), self.1.clone()))
    }
    fn restore(&mut self, checkpoint: Checkpoint) {
        let (mark, books) = *checkpoint
            .downcast::<MemoryMark>()
            .expect("memory checkpoint");
        self.0.rollback(mark);
        self.1 = books;
    }
    fn commit(&mut self, checkpoint: Checkpoint) {
        self.0.commit(
            checkpoint
                .downcast::<MemoryMark>()
                .expect("memory checkpoint")
                .0,
        );
    }
    fn validate_checkpoint(&self, checkpoint: &dyn std::any::Any) -> Result<()> {
        let (mark, books) = checkpoint
            .downcast_ref::<MemoryMark>()
            .expect("memory checkpoint");
        let mut changed = vec![];
        let facts_valid = self.0.changed_pairs_since(mark).all(|(p, f, old)| {
            changed.push(*p);
            f.valid_confidence(old.map(|f| f.as_facts()))
                && (old.is_some_and(|old| f.same_keys(old)) || self.keys_match(p, f))
        });
        changed.extend(self.0.removed_since(mark).copied());
        changed.extend(
            self.1
                .changed_pairs(books)
                .filter(|(_, b, old)| old.is_none_or(|old| !b.entries.ptr_eq(&old.entries)))
                .map(|(p, _, _)| *p),
        );
        changed.sort_unstable();
        changed.dedup();
        require(
            facts_valid
                && changed.iter().all(|p| {
                    self.1.get(p).map_or_else(
                        || self.0.get(p).is_none_or(|f| f.is_empty()),
                        |book| self.valid_book(p, book, books.get(p)),
                    )
                })
                && books.changed(&self.1).all(|(p, _)| {
                    self.1.contains_key(p) || self.0.get(p).is_none_or(|f| f.is_empty())
                }),
            "invalid personal memory",
        )
    }
}
