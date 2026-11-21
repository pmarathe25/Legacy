//! Summarize inherited traits weighted by physically held seed quantities.
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

struct Lineages<'a> {
    founders: &'a [Value],
    descendants: &'a [Value],
}

impl<'a> Lineages<'a> {
    fn find(&self, batch: u64) -> Option<&'a Value> {
        fn lookup(rows: &[Value], batch: u64) -> Option<&Value> {
            let index = rows
                .binary_search_by_key(&batch, |row| row["batch"].as_u64().unwrap_or(0))
                .ok()?;
            rows.get(index)
        }
        lookup(self.founders, batch).or_else(|| {
            let latest = self.descendants.partition_point(|chunk| {
                chunk[0]["batch"]
                    .as_u64()
                    .is_some_and(|first| first <= batch)
            });
            let rows = self.descendants.get(latest.checked_sub(1)?)?.as_array()?;
            lookup(rows, batch)
        })
    }
}

#[derive(Default)]
struct Sample {
    units: u128,
    units_with_genetics: u128,
    allele_sum: u128,
    cross_pollinated_units: u128,
    batches: BTreeSet<u64>,
}

impl Sample {
    fn observe(&mut self, lot: &Value, lineages: &Lineages<'_>) {
        let Some(quantity) = lot["quantity"].as_u64().map(u128::from) else {
            return;
        };
        self.units += quantity;
        let Some(batch) = lot["batch"].as_u64() else {
            return;
        };
        let Some(row) = lineages.find(batch) else {
            return;
        };
        let (Some(left), Some(right)) = (row["alleles"][0].as_u64(), row["alleles"][1].as_u64())
        else {
            return;
        };
        self.batches.insert(batch);
        self.units_with_genetics += quantity;
        self.allele_sum += (u128::from(left) + u128::from(right)) * quantity;
        if row["pollen_parent"].as_u64().is_some() && row["pollen_parent"] != row["parent"] {
            self.cross_pollinated_units += quantity;
        }
    }

    fn report(&self) -> Value {
        json!({
            "units": u64::try_from(self.units).ok(),
            "units_with_genetics": u64::try_from(self.units_with_genetics).ok(),
            "identified_batches": self.batches.len(),
            "cross_pollinated_units": u64::try_from(self.cross_pollinated_units).ok(),
            "allele_mean": (self.units_with_genetics > 0)
                .then(|| self.allele_sum as f64 / (2.0 * self.units_with_genetics as f64)),
        })
    }
}

pub(super) fn sample(snapshot: &Value) -> Vec<Value> {
    let agriculture = &snapshot["agriculture"];
    let provenance = &agriculture["provenance"];
    let genetics = &provenance["genetics"];
    if genetics.is_null() {
        return Vec::new();
    }
    let entries = |field| genetics[field].as_array().map(Vec::as_slice).unwrap_or(&[]);
    let lineages = Lineages {
        founders: entries("strains"),
        descendants: entries("descendants"),
    };
    let mut seeds: BTreeMap<u64, (Sample, Sample)> = agriculture["crops"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|crop| crop["seed"].as_u64())
        .map(|seed| (seed, (Sample::default(), Sample::default())))
        .collect();
    let physical = &snapshot["core"]["physical"];
    for holder in physical["custody"]["containers"]
        .as_array()
        .into_iter()
        .flatten()
    {
        for lot in holder["lots"].as_array().into_iter().flatten() {
            if let Some((stored, _)) = lot["good"].as_u64().and_then(|good| seeds.get_mut(&good)) {
                stored.observe(lot, &lineages);
            }
        }
    }
    for planting in provenance["plantings"].as_array().into_iter().flatten() {
        for lot in planting["seeds"].as_array().into_iter().flatten() {
            if let Some((_, planted)) = lot["good"].as_u64().and_then(|good| seeds.get_mut(&good)) {
                planted.observe(lot, &lineages);
            }
        }
    }
    let names: BTreeMap<_, _> = physical["goods"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|good| Some((good["id"].as_u64()?, good["name"].as_str()?)))
        .collect();
    seeds.into_iter()
        .filter(|(_, (stored, planted))| stored.units_with_genetics > 0 || planted.units_with_genetics > 0)
        .map(|(seed, (stored, planted))| {
            json!({"good": seed, "name": names.get(&seed), "stored": stored.report(), "planted": planted.report()})
        }).collect()
}

pub(super) fn sample_wild(snapshot: &Value) -> Vec<Value> {
    snapshot["gathering"]["wild_plants"]["stands"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|stand| {
            let node = stand["node"].as_u64()?;
            let cohorts: Vec<_> = stand["cohorts"]
                .as_array()?
                .iter()
                .filter_map(|cohort| {
                    Some(json!({
                        "batch": cohort["strain"]["batch"].as_u64()?,
                        "alleles": cohort["strain"]["alleles"].as_array()?,
                        "adults": cohort["adults"].as_u64()?,
                        "seeds": cohort["seeds"].as_u64()?,
                    }))
                })
                .collect();
            Some(json!({"node": node, "cohorts": cohorts}))
        })
        .collect()
}

pub(super) fn write_wild_history(text: &mut String, years: &Value) {
    let mut observed = BTreeMap::new();
    for row in years.as_array().into_iter().flatten() {
        let Some(year) = row["year"].as_u64() else {
            continue;
        };
        for stand in row["wild_plants"].as_array().into_iter().flatten() {
            let Some(node) = stand["node"].as_u64() else {
                continue;
            };
            for cohort in stand["cohorts"].as_array().into_iter().flatten() {
                if let Some(batch) = cohort["batch"].as_u64() {
                    let sample = (year, cohort);
                    let range = observed.entry((node, batch)).or_insert((sample, sample));
                    range.1 = sample;
                }
            }
        }
    }
    if observed.is_empty() {
        return;
    }
    text.push_str("## Wild plant populations\n\n");
    for ((node, batch), ((first_year, opening), (year, latest))) in observed.into_iter().take(12) {
        let alleles = &latest["alleles"];
        let adults = &latest["adults"];
        let seeds = &latest["seeds"];
        if first_year == year {
            writeln!(text, "- Stand {node}, strain {batch} {alleles}, held {adults} adult plants and {seeds} viable seeds in year {year}.").unwrap();
        } else {
            let before_adults = &opening["adults"];
            let before_seeds = &opening["seeds"];
            writeln!(text, "- Stand {node}, strain {batch} {alleles}, had {before_adults} adults and {before_seeds} viable seeds in year {first_year}. In year {year} it had {adults} adults and {seeds} viable seeds.").unwrap();
        }
    }
    text.push('\n');
}

pub(super) fn write_history(text: &mut String, years: &Value) {
    let mut observed = BTreeMap::new();
    for row in years.as_array().into_iter().flatten() {
        let Some(year) = row["year"].as_u64() else {
            continue;
        };
        for seed in row["seed_lineages"].as_array().into_iter().flatten() {
            if let (Some(good), Some(mean)) = (
                seed["good"].as_u64(),
                seed["stored"]["allele_mean"].as_f64(),
            ) {
                let sample = (year, mean, seed);
                let range = observed.entry(good).or_insert((sample, sample));
                range.1 = sample;
            }
        }
    }
    if observed.is_empty() {
        return;
    }
    text.push_str("## Cultivated seed lineages\n\n");
    for ((first_year, first, _), (year, last, seed)) in observed.into_values().take(8) {
        let name = seed["name"].as_str().unwrap_or("Unnamed seed");
        let units = &seed["stored"]["units_with_genetics"];
        let batches = &seed["stored"]["identified_batches"];
        if first_year == year {
            writeln!(text, "- In year {year}, {name} had a mean tracked allele value of {last:.1} across {units} stored units in {batches} batches.").unwrap();
        } else {
            writeln!(text, "- The mean tracked allele value for {name} was {first:.1} in year {first_year} and {last:.1} in year {year}. The latest sample covers {units} stored units in {batches} batches.").unwrap();
        }
    }
    text.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_wild_adults_and_their_viable_seed_bank_separately() {
        let sample_for = |adults, seeds| {
            sample_wild(&json!({"gathering":{"wild_plants":{"stands":[
                {"node":7,"cohorts":[{"strain":{"batch":19,"alleles":[4,8]},"adults":adults,"seeds":seeds}]}
            ]}}}))
        };
        let samples = json!([
            {"year":0,"wild_plants":sample_for(5,2)},
            {"year":3,"wild_plants":sample_for(0,7)}
        ]);
        let mut report = String::new();
        write_wild_history(&mut report, &samples);
        assert!(report.contains("Stand 7, strain 19"));
        assert!(report.contains("5 adults and 2 viable seeds in year 0"));
        assert!(report.contains("In year 3 it had 0 adults and 7 viable seeds"));
    }

    #[test]
    fn samples_physical_cohorts_and_omits_extinct_lineages_from_current_averages() {
        let state = json!({
            "agriculture":{"crops":[{"seed":2}], "provenance":{
                "plantings":[{"seeds":[{"good":2,"batch":1,"quantity":3}]}],
                "genetics": {
                    "strains":[{"batch":1,"alleles":[200,400]},{"batch":2,"alleles":[0,0]}],
                    "descendants":[[{"batch":3,"parent":1,"pollen_parent":2,"alleles":[200,600]}]]
                }
            }},
            "core":{"physical":{"goods":[{"id":2,"name":"barley-seed"}],
                "custody":{"containers":[{"lots":[
                    {"good":2,"batch":3,"quantity":3},
                    {"good":2,"batch":1,"quantity":1},
                    {"good":2,"quantity":2}
                ]}]}
            }}
        });
        let groups = sample(&state);
        assert_eq!(groups.len(), 1);
        assert_eq!(
            groups[0]["stored"],
            json!({
                "units":6,"units_with_genetics":4,"identified_batches":2,"cross_pollinated_units":3,"allele_mean":375.0
            })
        );
        assert_eq!(groups[0]["planted"]["allele_mean"], 300.0);
        assert_eq!(groups[0]["name"], "barley-seed");
    }
}
