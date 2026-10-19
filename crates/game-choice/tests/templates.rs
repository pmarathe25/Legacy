use game_choice::*;
use game_sim::*;
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Sample(u64);
impl Request for Sample {
    const KEY: &'static str = "sample.template";
    type Response = ();
}
#[test]
fn immutable_templates_preserve_requests_but_do_not_share_motives_or_outlive_the_bound() {
    let mut cache = Templates::new(1);
    let mut first = cache
        .get(1, || {
            Proposal::new("one", motive(0, 0, 10, 0, 0), Sample(1))
        })
        .unwrap();
    first.motive.value = 999;
    let reused = cache
        .get(1, || {
            panic!("equal payload should reuse its validated template")
        })
        .unwrap();
    assert_eq!(reused.motive.value, 10);
    assert_eq!(reused.request, first.request);
    cache
        .get(2, || Proposal::new("two", motive(0, 0, 0, 0, 0), Sample(2)))
        .unwrap();
    let rebuilt = cache
        .get(1, || {
            Proposal::new("one", motive(0, 0, 20, 0, 0), Sample(1))
        })
        .unwrap();
    assert_eq!(rebuilt.motive.value, 20);
}
