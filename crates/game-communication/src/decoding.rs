//! Per-thread bounded projections; each entry checks the exact personal vocabulary.
use super::*;
use std::cell::RefCell;
thread_local! { static CACHE: RefCell<game_lexicon::Decoder> = RefCell::new(Default::default()); }
pub(super) fn meaning(c: &Communication, reader: PersonId, sounds: &[u8]) -> (Option<String>, u16) {
    c.lexicons.get(&reader).map_or((None, 0), |words| {
        CACHE.with(|cache| cache.borrow_mut().decode(reader, words, sounds))
    })
}
pub fn shared_meaning(
    c: &Communication,
    reader: PersonId,
    sounds: &[u8],
) -> Option<game_lexicon::Meaning> {
    let words = c.lexicons.get(&reader)?;
    CACHE.with(|cache| cache.borrow_mut().meaning(reader, words, sounds).0)
}
