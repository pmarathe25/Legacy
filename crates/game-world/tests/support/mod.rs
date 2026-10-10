use game_api::PersonId;
use serde_json::Value;

/// Locate the actual saved component payload; fail if an attack or fixture edit
/// targets an absent entity instead of silently adding an unused JSON field.
pub fn component<'a>(save: &'a mut Value, column: &str, person: PersonId) -> &'a mut Value {
    let rows = save["state"]["components"]["columns"][column]
        .as_array_mut()
        .expect("registered saved component column");
    let row = rows
        .iter_mut()
        .find(|row| row["entity"].as_u64() == Some(person.0))
        .expect("person has the saved component being edited");
    &mut row["value"]
}
