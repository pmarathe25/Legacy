//! Summaries of attributed actions in the archive, outside simulation state.
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fmt::Write;

#[derive(Default)]
pub(super) struct Chronicle {
    counts: BTreeMap<&'static str, u64>,
    people: BTreeMap<u64, BTreeMap<&'static str, u64>>,
    selected: Vec<Value>,
    previous_delivered: BTreeMap<u64, u64>,
    active_ties: BTreeMap<(u64, u64), u64>,
    repeated_ties: BTreeMap<(u64, u64), (u64, u64, u64)>,
}

#[derive(Clone, Copy)]
struct Receipt {
    label: &'static str,
    actor: Option<u64>,
    other: Option<u64>,
}

impl Chronicle {
    pub(super) fn snapshot(&mut self, state: &Value, year: u64, opening: bool) {
        let Some(contracts) = state
            .pointer("/agreements/contracts")
            .and_then(Value::as_array)
        else {
            return;
        };
        let mut now_active = BTreeMap::new();
        let mut observed = BTreeMap::new();
        for contract in contracts {
            let (Some(id), Some(delivered)) =
                (contract["id"].as_u64(), contract["delivered"].as_u64())
            else {
                continue;
            };
            observed.insert(id, delivered);
            if opening || delivered <= self.previous_delivered.get(&id).copied().unwrap_or(0) {
                continue;
            }
            let (Some(sender), Some(recipient)) =
                (contract["debtor"].as_u64(), contract["creditor"].as_u64())
            else {
                continue;
            };
            if sender != recipient {
                now_active.insert((sender, recipient), year);
            }
        }
        for &pair in now_active.keys() {
            if let Some(&earlier) = self.active_ties.get(&pair)
                && earlier.checked_add(1) == Some(year)
            {
                let entry = self
                    .repeated_ties
                    .entry(pair)
                    .or_insert((earlier, earlier, 1));
                entry.2 += 1 + u64::from(entry.1 != earlier);
                entry.1 = year;
            }
        }
        self.previous_delivered = observed;
        self.active_ties = now_active;
    }

    pub(super) fn observe(&mut self, event: &Value, year: u64, focus: Option<u64>) {
        let Some(kind) = event["kind"].as_str() else {
            return;
        };
        if kind == "site_changes" {
            for (field, label, accepted) in [
                (
                    "demolished",
                    "successfully demolished sites",
                    Some("destroyed"),
                ),
                ("salvaged", "accepted salvage transfers", Some("recovered")),
                ("released", "releases from destroyed barriers", None),
            ] {
                for row in event[field].as_array().into_iter().flatten() {
                    if accepted.is_none_or(|key| row[key].as_bool() == Some(true)) {
                        self.record(
                            event,
                            row,
                            year,
                            focus,
                            Receipt {
                                label,
                                actor: row["person"].as_u64(),
                                other: row["actor"].as_u64(),
                            },
                        );
                    }
                }
            }
            return;
        }
        if kind == "invocation_cost" {
            self.record(
                event,
                event,
                year,
                focus,
                Receipt {
                    label: "accepted invocation costs",
                    actor: event["person"].as_u64(),
                    other: None,
                },
            );
            return;
        }
        if kind == "tactical" {
            for row in event["sites"].as_array().into_iter().flatten() {
                let label = if row["collapsed"].as_bool() == Some(true) {
                    "tactical strikes on collapsing sites"
                } else {
                    "tactical strikes that damaged sites"
                };
                self.record(
                    event,
                    row,
                    year,
                    focus,
                    Receipt {
                        label,
                        actor: row["actor"].as_u64(),
                        other: None,
                    },
                );
            }
        }
        if !matches!(
            kind,
            "campaign"
                | "expeditions"
                | "tactical"
                | "confinement"
                | "mechanical_experiments"
                | "work_requests"
        ) {
            return;
        }
        for row in event["events"].as_array().into_iter().flatten() {
            if let Some(receipt) = classify(kind, row) {
                self.record(event, row, year, focus, receipt);
            }
        }
    }

    fn record(
        &mut self,
        event: &Value,
        row: &Value,
        year: u64,
        focus: Option<u64>,
        receipt: Receipt,
    ) {
        *self.counts.entry(receipt.label).or_default() += 1;
        if let Some(actor) = receipt.actor {
            *self
                .people
                .entry(actor)
                .or_default()
                .entry(receipt.label)
                .or_default() += 1;
        }
        if focus
            .is_some_and(|person| receipt.actor == Some(person) || receipt.other == Some(person))
        {
            self.selected.push(json!({
                "category": receipt.label, "year": year,
                "at_millis": row.get("at_millis").or_else(|| event.get("at_millis")),
                "actor": receipt.actor, "other": receipt.other, "receipt": row
            }));
        }
    }

    pub(super) fn summary(&self) -> Value {
        let mut contributors: Vec<_> = self.people.iter().collect();
        contributors.sort_unstable_by_key(|&(person, counts)| {
            (std::cmp::Reverse(counts.values().sum::<u64>()), *person)
        });
        contributors.truncate(10);
        let mut ties: Vec<_> = self.repeated_ties.iter().collect();
        ties.sort_unstable_by_key(|&(pair, &(_, _, years))| (std::cmp::Reverse(years), *pair));
        ties.truncate(10);
        json!({
            "counts": self.counts,
            "people": contributors.iter().map(|&(person, counts)| json!({"person":person,"counts":counts})).collect::<Vec<_>>(),
            "recurring_deliveries": ties.iter().map(|&(&(sender, recipient), &(first, latest, years))| {
                json!({"sender":sender,"recipient":recipient,"first_year":first,"latest_year":latest,"recorded_years":years})
            }).collect::<Vec<_>>()
        })
    }

    pub(super) fn selected(self) -> Vec<Value> {
        self.selected
    }
}

fn classify(kind: &str, row: &Value) -> Option<Receipt> {
    let person = row["person"].as_u64();
    let category = row["kind"].as_str().unwrap_or_default();
    let label = match (kind, category) {
        ("campaign", _) => match row["change"].as_str()? {
            "life_ended" => "incarnation deaths",
            "successor" => "incarnation successions",
            "guardian_met" => "guardian contacts",
            "mentorship_released" => "mentor release decisions",
            "control_suspended" => "accepted control suspensions",
            "control_restored" => "restorations of control",
            _ => return None,
        },
        ("expeditions", "reached_destination") => "expedition arrivals",
        ("expeditions", "supplies_acquired") => "expedition supplies acquired",
        ("expeditions", "objective_met") => "completed expedition objectives",
        ("expeditions", "reached_origin") => "expedition returns",
        ("expeditions", "abandoned") => "abandoned expeditions",
        ("tactical", "started") => "tactical encounters started",
        ("tactical", "withdrew") => "tactical withdrawals",
        ("confinement", "confined") => "captures",
        ("confinement", "released") => "releases from holds",
        ("mechanical_experiments", _) => match row["outcome"].as_str()? {
            "moved" => "mechanical tests that moved a load",
            "stalled" => "mechanical tests that stalled",
            "broke" => "mechanical tests that broke a part",
            _ => return None,
        },
        ("work_requests", _) if row["outcome"]["kind"] == "started" => {
            "work starts following requests"
        }
        _ => return None,
    };
    let primary = row["actor"]
        .as_u64()
        .or(person)
        .or_else(|| row["worker"].as_u64());
    let secondary = row["other"]
        .as_u64()
        .or_else(|| row["target"].as_u64())
        .or(person.filter(|id| Some(*id) != primary));
    Some(Receipt {
        label,
        actor: primary,
        other: secondary,
    })
}

pub(super) fn write_world(text: &mut String, chronicle: &Value) {
    if let Some(counts) = chronicle["counts"]
        .as_object()
        .filter(|counts| !counts.is_empty())
    {
        text.push_str("## Recorded activity\n\n");
        for (label, count) in counts {
            writeln!(text, "- {count} {label}.").unwrap();
        }
        text.push('\n');
    }
    if let Some(people) = chronicle["people"]
        .as_array()
        .filter(|people| !people.is_empty())
    {
        text.push_str("People with the most attributed actions in these categories include the following.\n\n");
        for person in people.iter().take(5) {
            let Some(counts) = person["counts"].as_object() else {
                continue;
            };
            let labels: Vec<_> = counts
                .iter()
                .map(|(label, count)| format!("{count} {label}"))
                .collect();
            writeln!(
                text,
                "- Person {} recorded {}.",
                person["person"],
                labels.join(", ")
            )
            .unwrap();
        }
        text.push('\n');
    }
    if let Some(ties) = chronicle["recurring_deliveries"]
        .as_array()
        .filter(|ties| !ties.is_empty())
    {
        text.push_str("## Recurring deliveries\n\n");
        for tie in ties.iter().take(5) {
            writeln!(text, "- Person {} repeatedly delivered goods to person {}. Recurring deliveries first appear in year {} and last appear in year {} across {} recorded years.",
                tie["sender"], tie["recipient"], tie["first_year"], tie["latest_year"], tie["recorded_years"]).unwrap();
        }
        text.push('\n');
    }
}

pub(super) fn write_person(text: &mut String, rows: &Value, time_name: fn(u64) -> String) {
    let Some(rows) = rows.as_array().filter(|rows| !rows.is_empty()) else {
        return;
    };
    text.push_str("## Selected recorded actions\n\n");
    for row in rows.iter().take(20) {
        let when = row["at_millis"]
            .as_u64()
            .map_or_else(|| format!("recorded year {}", row["year"]), time_name);
        let category = row["category"].as_str().unwrap_or_default();
        if let Some(site) = row["receipt"]["site"].as_u64() {
            writeln!(text, "- {when}, {category} at site {site}.").unwrap();
        } else if let Some(itinerary) = row["receipt"]["itinerary"].as_u64() {
            writeln!(text, "- {when}, {category} on itinerary {itinerary}.").unwrap();
        } else {
            writeln!(text, "- {when}, {category}.").unwrap();
        }
    }
    if rows.len() > 20 {
        writeln!(
            text,
            "\nThe archive contains {} additional attributed events for this person.",
            rows.len() - 20
        )
        .unwrap();
    }
    text.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarizes_consecutive_deliveries_and_attributes_only_accepted_recovery() {
        let mut history = Chronicle::default();
        let state = |delivered| {
            json!({
                "agreements":{"contracts":[{"id":1,"debtor":5,"creditor":6,"delivered":delivered}]}
            })
        };
        for (year, delivered) in [3, 4, 6, 6].into_iter().enumerate() {
            history.snapshot(&state(delivered), year as u64, year == 0);
        }
        assert_eq!(
            history.summary()["recurring_deliveries"],
            json!([
                {"sender":5,"recipient":6,"first_year":1,"latest_year":2,"recorded_years":2}
            ])
        );
        history.observe(&json!({
            "kind":"site_changes", "at_millis":172,
            "salvaged":[{"person":5,"site":4,"recovered":true}, {"person":5,"site":7,"recovered":false}]
        }), 1, Some(5));
        assert_eq!(history.summary()["counts"]["accepted salvage transfers"], 1);
        let events = history.selected();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["at_millis"], 172);
        assert_eq!(events[0]["receipt"]["site"], 4);
    }
}
