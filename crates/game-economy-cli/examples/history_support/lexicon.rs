//! Annual distributions of personal spoken and written forms.
use serde_json::{Value, json};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

#[derive(Default)]
pub(super) struct LexiconHistory {
    first: Option<Value>,
    latest: Option<Value>,
}

impl LexiconHistory {
    pub(super) fn observe(&mut self, state: &Value, year: u64) {
        let Some(language) = state
            .pointer("/social/language")
            .filter(|row| row.is_object())
        else {
            return;
        };
        let living: BTreeSet<_> = rows(&state["core"]["people"])
            .filter(|person| person["health"].as_u64().is_some_and(|health| health > 0))
            .filter_map(|person| person["id"].as_u64())
            .collect();
        let is_living = |mind: &&Value| {
            mind["person"]
                .as_u64()
                .is_some_and(|id| living.contains(&id))
        };
        let goods: BTreeMap<_, _> = rows(&state["core"]["physical"]["goods"])
            .filter_map(|good| Some((good["id"].as_u64()?, good["name"].as_str()?)))
            .collect();
        let mut speakers = 0;
        let mut writers = 0;
        let mut sounds: BTreeMap<(String, Vec<u64>), usize> = BTreeMap::new();
        let mut marks: BTreeMap<(Vec<u64>, Vec<u64>), usize> = BTreeMap::new();
        for mind in rows(&language["minds"]).filter(is_living) {
            let mut has_words = false;
            for word in rows(&mind["words"]) {
                if let Some(key) = referent(&word["meaning"], &goods).zip(ids(&word["sounds"])) {
                    has_words = true;
                    *sounds.entry(key).or_default() += 1;
                }
            }
            speakers += usize::from(has_words);
        }
        for mind in rows(&language["writing"]["minds"]).filter(is_living) {
            let mut has_marks = false;
            for form in rows(&mind["associations"]) {
                if let Some(key) = ids(&form["sounds"]).zip(ids(&form["marks"])) {
                    has_marks = true;
                    *marks.entry(key).or_default() += 1;
                }
            }
            writers += usize::from(has_marks);
        }
        let mut common: Vec<_> = sounds.iter().filter(|(_, count)| **count >= 2).collect();
        common.sort_unstable_by_key(|&(key, count)| (Reverse(*count), key));
        let most_shared: Vec<_> = common
            .iter()
            .take(8)
            .map(|&((meaning, form), count)| {
                let palette = &language["rules"]["sounds"];
                let symbols = form
                    .iter()
                    .map(|&sound| {
                        palette
                            .get(sound as usize)
                            .and_then(Value::as_str)
                            .map_or_else(|| format!("#{sound}"), str::to_owned)
                    })
                    .collect::<Vec<_>>()
                    .join(".");
                json!({"referent":meaning,"sounds":form,"pronunciation":symbols,"people":count})
            })
            .collect();
        let sample = json!({
            "year":year,
            "people_with_words":speakers,
            "people_with_written_forms":writers,
            "shared_spoken_forms":common.len(),
            "shared_written_forms":marks.values().filter(|&&count| count >= 2).count(),
            "most_shared":most_shared
        });
        if self.first.is_none() {
            self.first = Some(sample.clone());
        }
        self.latest = Some(sample);
    }

    pub(super) fn summary(&self) -> Value {
        json!({"first": self.first, "latest": self.latest})
    }
}

fn rows(value: &Value) -> impl Iterator<Item = &Value> {
    value.as_array().into_iter().flatten()
}

fn ids(value: &Value) -> Option<Vec<u64>> {
    let values = value.as_array()?;
    if values.is_empty() {
        return None;
    }
    values.iter().map(Value::as_u64).collect()
}

fn referent(value: &Value, goods: &BTreeMap<u64, &str>) -> Option<String> {
    match value["kind"].as_str()? {
        "object" => {
            let id = value["id"].as_u64()?;
            Some(
                goods
                    .get(&id)
                    .map_or_else(|| format!("object {id}"), |name| (*name).to_owned()),
            )
        }
        kind @ ("place" | "need" | "person" | "signal") => {
            Some(format!("{kind} {}", value["id"].as_u64()?))
        }
        "mechanical_result" => Some(format!(
            "apparatus {} outcome {}",
            value["id"]["apparatus"].as_u64()?,
            value["id"]["outcome"].as_u64()?
        )),
        _ => None,
    }
}

pub(super) fn write_history(text: &mut String, report: &Value) {
    let first = &report["first"];
    let latest = &report["latest"];
    if latest["year"].is_null()
        || latest["people_with_words"].as_u64() == Some(0)
            && first["people_with_words"].as_u64() == Some(0)
            && latest["people_with_written_forms"].as_u64() == Some(0)
            && first["people_with_written_forms"].as_u64() == Some(0)
    {
        return;
    }
    text.push_str("\n## Speech and writing\n\n");
    writeln!(
        text,
        "In year {}, {} living people held spoken forms. By year {}, {} held spoken forms and {} held written associations. The latest snapshot recorded {} spoken and {} written forms shared by at least two people.",
        first["year"], first["people_with_words"], latest["year"], latest["people_with_words"],
        latest["people_with_written_forms"], latest["shared_spoken_forms"], latest["shared_written_forms"]
    ).unwrap();
    text.push('\n');
    for row in rows(&latest["most_shared"]) {
        writeln!(
            text,
            "- {} people associated **{}** with {}.",
            row["people"],
            row["pronunciation"].as_str().unwrap_or_default(),
            row["referent"].as_str().unwrap_or_default(),
        )
        .unwrap();
    }
    text.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_forms_use_exact_personal_mappings_and_only_living_people() {
        let mut archive = LexiconHistory::default();
        let word = |index| json!({"meaning":{"kind":"place","id":1},"sounds":[index]});
        let sound = json!({"sounds":[0],"marks":[1]});
        let mut state = json!({
            "core":{"people":[{"id":1,"health":2},{"id":2,"health":3},{"id":3,"health":0}]},
            "social":{"language":{"rules":{"sounds":["a","ku"]},
                "minds":[{"person":1,"words":[word(0)]},{"person":2,"words":[word(1)]},{"person":3,"words":[word(0)]}],
                "writing":{"minds":[{"person":1,"associations":[&sound]},{"person":3,"associations":[&sound]}]}
            }}
        });
        archive.observe(&state, 0);
        state["social"]["language"]["minds"][1]["words"][0] = word(0);
        archive.observe(&state, 4);
        let summary = archive.summary();
        assert_eq!(summary["first"]["shared_spoken_forms"], 0);
        assert_eq!(summary["latest"]["people_with_words"], 2);
        assert_eq!(summary["latest"]["shared_written_forms"], 0);
        assert_eq!(summary["latest"]["most_shared"][0]["people"], 2);
        let mut text = String::new();
        write_history(&mut text, &summary);
        assert!(text.contains("2 people associated **a** with place 1."));
    }
}
