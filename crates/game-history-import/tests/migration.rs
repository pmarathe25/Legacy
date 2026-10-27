use game_history_import::*;
use game_history_kernel::*;
#[test]
fn legacy_audit_rejects_durable_jobs_and_unknown_owners() {
    let text=serde_json::json!({"version":1,"time":60000,"seed":1,"jobs":[{"key":"birth"}],"resources":{"physiology.bodies":{"people":{}},"transport.positions":{},"land.areas":{"1":{"name":"A"}},"culture.memories":{"person":17}}}).to_string();
    let report = inspect_legacy(&text).unwrap();
    assert!(!report.supported);
    assert!(report.issues.iter().any(|i| i.path == "jobs"));
    assert!(
        report
            .issues
            .iter()
            .any(|i| i.path == "resources.culture.memories")
    );
    assert!(import_legacy(&text).is_err());
}
#[test]
fn physical_legacy_subset_preserves_ids_and_time_and_reloads() {
    let text=serde_json::json!({"version":1,"time":60000*30*365*DAY,"seed":1,"jobs":[],"resources":{"physiology.bodies":{"people":{"17":{"kind":"Mortal","born":60000*10*365*DAY,"alive":true,"hunger":30,"fatigue":20,"health":100}}},"transport.positions":{"17":{"At":9}},"land.areas":{"9":{"name":"Imported"}},"world.actions":{},"cargo.inventories":{}}}).to_string();
    let w = import_legacy(&text).unwrap();
    assert_eq!(w.population().people()[0].id, 17);
    assert_eq!(w.population().people()[0].area, 9);
    assert_eq!(w.now, 30 * 365 * DAY);
    assert_eq!(w, World::load(&w.save().unwrap()).unwrap());
}
