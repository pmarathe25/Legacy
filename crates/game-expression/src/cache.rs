//! Derived values use exact life, clock, key and raw-potential inputs, including within transactions.
use super::*;
use std::cell::RefCell;
struct Frame {
    person: PersonId,
    at: SimTime,
    life: Life,
    seen: u32,
    raw: [u64; 32],
    values: [u64; 32],
}
thread_local! {
    static CACHE: RefCell<Box<[Option<Frame>; 128]>> =
        RefCell::new(Box::new(std::array::from_fn(|_| None)));
}
pub fn ability(life: &Life, person: PersonId, key: &str, at: SimTime, raw: u64) -> u64 {
    let Some(index) = ability_slot(key) else {
        return raw;
    };
    remember(life, person, at, index, raw, || life.ability(raw, key, at))
}
pub fn disposition(life: &Life, person: PersonId, key: &str, at: SimTime, raw: i64) -> i64 {
    let Some(index) = disposition_slot(key) else {
        return raw;
    };
    remember(life, person, at, index, raw as u64, || {
        life.disposition(raw, key, at) as u64
    }) as i64
}
fn remember(
    life: &Life,
    person: PersonId,
    at: SimTime,
    index: usize,
    raw: u64,
    compute: impl FnOnce() -> u64,
) -> u64 {
    if index >= 32 {
        return compute();
    }
    CACHE.with(|slot| {
        let mut cache = slot.borrow_mut();
        let row = &mut cache[person.0 as usize % 128];
        if row
            .as_ref()
            .is_none_or(|f| f.person != person || f.at != at || f.life != *life)
        {
            *row = Some(Frame {
                person,
                at,
                life: *life,
                seen: 0,
                raw: [0; 32],
                values: [0; 32],
            });
        }
        let frame = row.as_mut().unwrap();
        if frame.seen & (1 << index) == 0 || frame.raw[index] != raw {
            frame.raw[index] = raw;
            frame.values[index] = compute();
            frame.seen |= 1 << index;
        }
        frame.values[index]
    })
}
