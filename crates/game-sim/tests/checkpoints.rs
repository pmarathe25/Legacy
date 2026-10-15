use game_sim::*;
use serde::{Deserialize, Serialize};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
#[derive(Default, Serialize, Deserialize)]
struct History {
    values: Vec<u64>,
    #[serde(skip)]
    copies: Arc<AtomicUsize>,
}
impl Clone for History {
    fn clone(&self) -> Self {
        self.copies.fetch_add(1, Ordering::Relaxed);
        Self {
            values: self.values.clone(),
            copies: self.copies.clone(),
        }
    }
}
impl Resource for History {
    const KEY: &'static str = "checkpoint.history";
}
#[derive(Clone, Serialize, Deserialize)]
struct Empty;
impl Request for Empty {
    const KEY: &'static str = "checkpoint.empty";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Run;
impl Request for Run {
    const KEY: &'static str = "checkpoint.run";
    type Response = ();
}
#[test]
fn nested_attempts_do_not_clone_resources_they_never_edit() {
    let mut b = Builder::new();
    b.plugin("checkpoint", 1).unwrap();
    b.resource::<History>().unwrap();
    b.request::<Empty>(|_, _| Ok(())).unwrap();
    b.request::<Run>(|tx, _| {
        tx.edit::<History>()?.values.extend(0..20_000);
        for _ in 0..100 {
            tx.attempt(Empty)?;
        }
        Ok(())
    })
    .unwrap();
    let mut host = b.build(1, 1).unwrap();
    let copies = host.get::<History>().unwrap().copies.clone();
    host.dispatch(Run).unwrap();
    assert_eq!(
        copies.load(Ordering::Relaxed),
        1,
        "only the initial write copies this resource"
    );
    assert_eq!(host.get::<History>().unwrap().values.len(), 20_000);
}
