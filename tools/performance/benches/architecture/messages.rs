//! Runtime/wire separation, with the real host Envelope as the JSON baseline.
use game_core::*;
use game_sim::Envelope;
use serde::{Deserialize, Serialize, ser::SerializeStruct};
use std::{sync::Arc, time::Instant};
const KEY: &str = "physiology.cancel_rest";
#[derive(Clone, Serialize, Deserialize)]
struct Cancel {
    person: PersonId,
    job: ScheduleId,
}
trait Payload: Send + Sync {
    fn encode(&self) -> serde_json::Value;
    fn invoke(&self) -> u64;
}
impl Payload for Cancel {
    fn encode(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap()
    }
    fn invoke(&self) -> u64 {
        let r = self.clone();
        r.person.0 ^ r.job.0
    }
}
#[derive(Clone)]
struct Packet {
    key: &'static str,
    payload: Arc<dyn Payload>,
}
impl Serialize for Packet {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let mut state = s.serialize_struct("Envelope", 2)?;
        state.serialize_field("key", self.key)?;
        state.serialize_field("payload", &self.payload.encode())?;
        state.end()
    }
}
fn cancel(i: usize) -> Cancel {
    Cancel {
        person: PersonId(3 + i as u64 * 7),
        job: ScheduleId(1_000_003 + i as u64 * 13),
    }
}
fn milliseconds(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.
}
pub fn measure(n: usize) {
    let reference: u64 = (0..n)
        .map(|i| {
            let r = cancel(i);
            r.person.0 ^ r.job.0
        })
        .sum();
    let sample = game_performance::metrics::Sample::new();
    let t = Instant::now();
    let pending: Vec<_> = (0..n)
        .map(|i| Envelope {
            key: KEY.into(),
            payload: serde_json::to_value(cancel(i)).unwrap(),
        })
        .collect();
    let construct = milliseconds(t);
    sample.report("json_pending_construct");
    let t = Instant::now();
    let result: u64 = pending
        .iter()
        .map(|p| {
            let owned = p.clone();
            assert_eq!(owned.key, KEY);
            let r: Cancel = serde_json::from_value(owned.payload).unwrap();
            r.person.0 ^ r.job.0
        })
        .sum();
    let invoke = milliseconds(t);
    assert_eq!(result, reference);
    let t = Instant::now();
    let bytes = serde_json::to_vec(&pending).unwrap();
    let encode = milliseconds(t);
    let hash = game_performance::hash(&bytes);
    let size = bytes.len();
    let t = Instant::now();
    let loaded: Vec<Envelope> = serde_json::from_slice(&bytes).unwrap();
    let decode = milliseconds(t);
    assert_eq!(pending, loaded);
    println!(
        "queued_layout=json population={n} construct_ms={construct:.3} invoke_ms={invoke:.3} encode_ms={encode:.3} decode_ms={decode:.3} bytes={size} hash={hash:016x}"
    );
    drop((pending, loaded, bytes));
    let sample = game_performance::metrics::Sample::new();
    let t = Instant::now();
    let pending: Vec<_> = (0..n)
        .map(|i| {
            let request = cancel(i);
            serde_json::to_writer(std::io::sink(), &request).unwrap();
            Packet {
                key: KEY,
                payload: Arc::new(request),
            }
        })
        .collect();
    let construct = milliseconds(t);
    sample.report("typed_pending_construct");
    let t = Instant::now();
    let result: u64 = pending
        .iter()
        .map(|p| {
            let owned = p.clone();
            assert_eq!(owned.key, KEY);
            owned.payload.invoke()
        })
        .sum();
    let invoke = milliseconds(t);
    assert_eq!(result, reference);
    let t = Instant::now();
    let bytes = serde_json::to_vec(&pending).unwrap();
    let encode = milliseconds(t);
    assert_eq!(bytes.len(), size);
    assert_eq!(game_performance::hash(&bytes), hash);
    let t = Instant::now();
    let wire: Vec<Envelope> = serde_json::from_slice(&bytes).unwrap();
    let loaded: Vec<_> = wire
        .into_iter()
        .enumerate()
        .map(|(i, p)| {
            assert_eq!(p.key, KEY);
            let r: Cancel = serde_json::from_value(p.payload).unwrap();
            let expected = cancel(i);
            assert_eq!((r.person, r.job), (expected.person, expected.job));
            Packet {
                key: KEY,
                payload: Arc::new(r),
            }
        })
        .collect();
    let decode = milliseconds(t);
    assert_eq!(
        loaded.iter().map(|p| p.payload.invoke()).sum::<u64>(),
        reference
    );
    println!(
        "queued_layout=typed population={n} construct_ms={construct:.3} invoke_ms={invoke:.3} encode_ms={encode:.3} decode_ms={decode:.3} bytes={size} hash={hash:016x}"
    );
}
