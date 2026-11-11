//! Inspect streamed annual history without treating inferred groups as simulation entities.
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

#[derive(Clone, Copy)]
enum Focus {
    Summary,
    Person(u64),
    Animal(u64),
}

fn id(row: &Value) -> Option<u64> {
    row.get("id")?.as_u64()
}
fn genome_means<'a>(genomes: impl Iterator<Item = &'a Value>) -> Option<Vec<f64>> {
    let mut sums = [0.0; 4];
    let mut count = 0u64;
    for genome in genomes {
        let alleles = genome.get("alleles")?.as_array()?;
        if alleles.len() != 4 {
            return None;
        }
        for (index, locus) in alleles.iter().enumerate() {
            let pair = locus.as_array()?;
            if pair.len() != 2 {
                return None;
            }
            sums[index] += (pair[0].as_f64()? + pair[1].as_f64()?) / 2.0;
        }
        count += 1;
    }
    (count > 0).then(|| sums.into_iter().map(|sum| sum / count as f64).collect())
}
fn mate_desire_mean<'a>(genomes: impl Iterator<Item = &'a Value>) -> Option<f64> {
    let (mut sum, mut count) = (0.0, 0u64);
    for genome in genomes {
        // Earlier snapshots predate the separate locus and load as neutral.
        let value = match genome.get("mate_desire") {
            None => 500.0,
            Some(pair) => {
                let pair = pair.as_array()?;
                (pair.first()?.as_f64()? + pair.get(1)?.as_f64()?) / 2.0
            }
        };
        sum += value;
        count += 1;
    }
    (count > 0).then_some(sum / count as f64)
}
fn animal_species_samples(animals: &[Value], species: &[Value]) -> Vec<Value> {
    let mut groups: BTreeMap<u64, Vec<&Value>> = BTreeMap::new();
    for definition in species {
        if let Some(id) = definition["id"].as_u64() {
            groups.entry(id).or_default();
        }
    }
    for animal in animals {
        if animal["health"].as_u64().unwrap_or(0) > 0
            && let Some(id) = animal["species"].as_u64()
        {
            groups.entry(id).or_default().push(animal);
        }
    }
    groups.into_iter().map(|(id, members)| {
        let name = species.iter().find(|row| row["id"].as_u64() == Some(id)).map_or(Value::Null, |row| row["name"].clone());
        let means = genome_means(members.iter().map(|animal| &animal["genome"]));
        let desire = mate_desire_mean(members.iter().map(|animal| &animal["genome"]));
        let mut regions: BTreeMap<u64, u64> = BTreeMap::new();
        for animal in &members {
            if let Some(region) = animal["region"].as_u64() {
                *regions.entry(region).or_default() += 1;
            }
        }
        json!({"species":id,"name":name,"living":members.len(),"regions":regions,"locus_means":means,"mate_desire_mean":desire})
    }).collect()
}

fn inspect(path: &Path, focus: Focus) -> Result<Value, String> {
    let input = BufReader::new(File::open(path).map_err(|e| e.to_string())?);
    let mut metadata = Value::Null;
    let mut metadata_corrections = Vec::new();
    let mut complete = false;
    let mut expected_years = None;
    let mut completed_years = None;
    let mut years = 0u64;
    let mut first_year = None;
    let mut last_year = None;
    let mut peak_people = (0u64, 0u64);
    let mut peak_animals = (0u64, 0u64);
    let mut human_zero = None;
    let mut animal_zero = None;
    let mut species_history: BTreeMap<u64, (Value, u64, u64, Option<u64>)> = BTreeMap::new();
    let mut totals = Value::Null;
    let mut samples = Vec::new();
    let mut meetings: BTreeMap<u64, u64> = BTreeMap::new();
    let mut meeting_count = 0u64;
    let mut focal_meetings = Vec::new();
    let mut focal_first = None;
    let mut focal_last_alive = None;
    let mut focal_last = None;
    let mut focal_person_state = None;
    let mut focal_events = Vec::new();
    let mut focal_opinions = Vec::new();
    let mut previous_year = None;
    for (index, line) in input.lines().enumerate() {
        let line = line.map_err(|e| e.to_string())?;
        if line.trim().is_empty() {
            continue;
        }
        let row: Value =
            serde_json::from_str(&line).map_err(|e| format!("line {}: {e}", index + 1))?;
        if complete {
            return Err("history contains data after its completion record".into());
        }
        match row.get("kind").and_then(Value::as_str) {
            Some("metadata") => {
                if !metadata.is_null() {
                    return Err("history has duplicate metadata".into());
                }
                if row["limitations"].as_array().is_some_and(|items| {
                    items.iter().any(|item| {
                        item.as_str()
                            == Some("animal parentage is not recorded by the current fauna owner")
                    })
                }) {
                    metadata_corrections.push("The source metadata incorrectly says animal parentage is absent. Animals born in these runs have recorded parent IDs; founders may not.");
                }
                expected_years = row["years_requested"].as_u64();
                metadata = json!({"calendar":row["calendar"],"tick_millis":row["tick_millis"],"ticks_per_year":row["ticks_per_year"],"seed":row["seed"],"git_revision":row["git_revision"],"git_revision_scope":row["git_revision_scope"],"tracked_source_modified":row["tracked_source_modified"],"executable":row["executable"],"source_kind":row["source_kind"],"limitations":row["limitations"]});
            }
            Some("complete") => {
                complete = true;
                completed_years = row["years"].as_u64();
            }
            Some("initial" | "year") => {
                let year = row
                    .get("year")
                    .and_then(Value::as_u64)
                    .ok_or("history row has no year")?;
                if previous_year.is_some_and(|old: u64| old.checked_add(1) != Some(year)) {
                    return Err("history years are not consecutive".into());
                }
                previous_year = Some(year);
                first_year.get_or_insert(year);
                last_year = Some(year);
                if row["kind"] == "year" {
                    years += 1;
                }
                let state = row.get("snapshot").ok_or("history row has no snapshot")?;
                let people = state.pointer("/family/members").and_then(Value::as_array);
                let fauna = state.pointer("/fauna/animals").and_then(Value::as_array);
                let live_people = state
                    .pointer("/core/people")
                    .and_then(Value::as_array)
                    .ok_or("history snapshot has no people")?
                    .iter()
                    .filter(|person| person["health"].as_u64().unwrap_or(0) > 0)
                    .count() as u64;
                let live_animals = fauna.map_or(0, |items| {
                    items
                        .iter()
                        .filter(|animal| animal["health"].as_u64().unwrap_or(0) > 0)
                        .count() as u64
                });
                if matches!(focus, Focus::Summary)
                    && let Some(definitions) =
                        state.pointer("/fauna/species").and_then(Value::as_array)
                    && let Some(animals) = fauna
                {
                    let mut counts: BTreeMap<u64, u64> = BTreeMap::new();
                    for animal in animals
                        .iter()
                        .filter(|animal| animal["health"].as_u64().unwrap_or(0) > 0)
                    {
                        if let Some(species) = animal["species"].as_u64() {
                            *counts.entry(species).or_default() += 1;
                        }
                    }
                    for definition in definitions {
                        if let Some(id) = definition["id"].as_u64() {
                            let count = counts.get(&id).copied().unwrap_or(0);
                            let entry = species_history.entry(id).or_insert((
                                definition["name"].clone(),
                                year,
                                0,
                                None,
                            ));
                            if count > entry.2 {
                                entry.1 = year;
                                entry.2 = count;
                            }
                            if count == 0 && entry.3.is_none() {
                                entry.3 = Some(year);
                            }
                        }
                    }
                }
                if live_people > peak_people.1 {
                    peak_people = (year, live_people);
                }
                if live_animals > peak_animals.1 {
                    peak_animals = (year, live_animals);
                }
                if live_people == 0 && human_zero.is_none() {
                    human_zero = Some(year);
                }
                if live_animals == 0 && animal_zero.is_none() {
                    animal_zero = Some(year);
                }
                if !state["family"].is_null() {
                    totals = state["family"]["totals"].clone();
                }
                if year % 50 == 0 || first_year == Some(year) {
                    let human_means = people.and_then(|items| {
                        genome_means(
                            items
                                .iter()
                                .filter(|member| member["alive"] == true)
                                .map(|member| &member["genome"]),
                        )
                    });
                    let animal_means = fauna.and_then(|items| {
                        genome_means(
                            items
                                .iter()
                                .filter(|animal| animal["health"].as_u64().unwrap_or(0) > 0)
                                .map(|animal| &animal["genome"]),
                        )
                    });
                    let human_desire = people.and_then(|items| {
                        mate_desire_mean(
                            items
                                .iter()
                                .filter(|member| member["alive"] == true)
                                .map(|member| &member["genome"]),
                        )
                    });
                    let animal_desire = fauna.and_then(|items| {
                        mate_desire_mean(
                            items
                                .iter()
                                .filter(|animal| animal["health"].as_u64().unwrap_or(0) > 0)
                                .map(|animal| &animal["genome"]),
                        )
                    });
                    let animal_species = fauna.map(|items| {
                        animal_species_samples(
                            items,
                            state
                                .pointer("/fauna/species")
                                .and_then(Value::as_array)
                                .map_or(&[][..], Vec::as_slice),
                        )
                    });
                    samples.push(json!({"year":year,"living_people":live_people,"living_animals":live_animals,"human_locus_means":human_means,"animal_locus_means":animal_means,"human_mate_desire_mean":human_desire,"animal_mate_desire_mean":animal_desire,"animal_species":animal_species}));
                }
                if row["kind"] == "year" {
                    if let Some(events) = row["observations"].as_array() {
                        for event in events {
                            if event["kind"] == "animal_population" {
                                if let Focus::Animal(animal) = focus {
                                    let appeared =
                                        event["appeared"].as_array().is_some_and(|rows| {
                                            rows.iter().any(|row| id(row) == Some(animal))
                                        });
                                    let inactive =
                                        event["inactive"].as_array().is_some_and(|rows| {
                                            rows.iter().any(|row| row.as_u64() == Some(animal))
                                        });
                                    if appeared || inactive {
                                        focal_events.push(event.clone());
                                    }
                                }
                                continue;
                            }
                            if event["kind"] == "family_events" {
                                if let Focus::Person(person) = focus
                                    && let Some(rows) = event["events"].as_array()
                                {
                                    focal_events.extend(
                                        rows.iter()
                                            .filter(|row| {
                                                row["people"].as_array().is_some_and(|people| {
                                                    people
                                                        .iter()
                                                        .any(|id| id.as_u64() == Some(person))
                                                })
                                            })
                                            .cloned(),
                                    );
                                }
                                continue;
                            }
                            if event["kind"] == "opinion_changes" {
                                if let Focus::Person(person) = focus
                                    && let Some(rows) = event["events"].as_array()
                                {
                                    focal_opinions.extend(
                                        rows.iter()
                                            .filter(|row| {
                                                row["observer"].as_u64() == Some(person)
                                                    || row["target"].as_u64() == Some(person)
                                            })
                                            .cloned(),
                                    );
                                }
                                continue;
                            }
                            if event["kind"] != "meetings" {
                                continue;
                            }
                            if let Some(encounters) = event["events"].as_array() {
                                for encounter in encounters {
                                    let (Some(a), Some(b)) =
                                        (encounter["a"].as_u64(), encounter["b"].as_u64())
                                    else {
                                        return Err("invalid meeting identity".into());
                                    };
                                    *meetings.entry(a).or_default() += 1;
                                    *meetings.entry(b).or_default() += 1;
                                    meeting_count += 1;
                                    if matches!(focus,Focus::Person(person) if person==a||person==b)
                                    {
                                        focal_meetings.push(encounter.clone());
                                    }
                                }
                            }
                        }
                    }
                    if !row["observations"].as_array().is_some_and(|events| {
                        events.iter().any(|event| event["kind"] == "family_events")
                    }) && let Focus::Person(person) = focus
                        && let Some(events) =
                            state.pointer("/family/events").and_then(Value::as_array)
                    {
                        focal_events.extend(
                            events
                                .iter()
                                .filter(|event| {
                                    event["people"].as_array().is_some_and(|people| {
                                        people.iter().any(|id| id.as_u64() == Some(person))
                                    })
                                })
                                .cloned(),
                        );
                    }
                }
                match focus {
                    Focus::Person(person) => {
                        if let Some(individual) = state
                            .pointer("/core/people")
                            .and_then(Value::as_array)
                            .and_then(|items| items.iter().find(|item| id(item) == Some(person)))
                        {
                            focal_first.get_or_insert(year);
                            if individual["health"].as_u64().unwrap_or(0) > 0 {
                                focal_last_alive = Some(year);
                            }
                            focal_person_state = Some(individual.clone());
                        }
                        if let Some(member) = people.and_then(|items| {
                            items.iter().find(|member| id(member) == Some(person))
                        }) {
                            focal_last = Some(member.clone());
                        }
                    }
                    Focus::Animal(animal) => {
                        if let Some(item) = fauna
                            .and_then(|items| items.iter().find(|item| id(item) == Some(animal)))
                        {
                            focal_first.get_or_insert(year);
                            if item["health"].as_u64().unwrap_or(0) > 0 {
                                focal_last_alive = Some(year);
                            }
                            focal_last = Some(item.clone());
                        }
                    }
                    Focus::Summary => {}
                }
            }
            Some("failure") => {
                return Err(format!(
                    "history reports failure in year {}: {}",
                    row["year"], row["message"]
                ));
            }
            _ => {}
        }
    }
    if metadata.is_null() || first_year.is_none() {
        return Err("history has no metadata or initial state".into());
    }
    if complete
        && (expected_years.is_some_and(|expected| expected != years)
            || completed_years.is_some_and(|completed| completed != years))
    {
        return Err("history completion count does not match annual records".into());
    }
    let mut ranking: Vec<_> = meetings.into_iter().collect();
    ranking.sort_unstable_by_key(|&(person, count)| (std::cmp::Reverse(count), person));
    ranking.truncate(10);
    let scope = json!({"source":path,"complete":complete,"first_year":first_year,"last_year":last_year,"annual_records":years,"metadata":metadata,"metadata_corrections":metadata_corrections});
    match focus {
        Focus::Summary => Ok(
            json!({"scope":scope,"peak_people":{"year":peak_people.0,"count":peak_people.1},"peak_animals":{"year":peak_animals.0,"count":peak_animals.1},"first_zero_people":human_zero,"first_zero_animals":animal_zero,"animal_species_history":species_history.iter().map(|(id,(name,year,count,zero))|json!({"species":id,"name":name,"peak":{"year":year,"count":count},"first_zero":zero})).collect::<Vec<_>>(),"family_totals":totals,"recorded_meetings":meeting_count,"most_recorded_meetings":ranking.iter().map(|(person,count)|json!({"person":person,"count":count})).collect::<Vec<_>>(),"genome_samples":samples,"interpretation":"Meeting counts do not establish leadership; changes in allele means do not establish selection or causation."}),
        ),
        Focus::Person(person) => {
            if focal_first.is_none() {
                return Err(format!("person {person} was not found in this history"));
            }
            Ok(
                json!({"scope":scope,"person":person,"first_observed_year":focal_first,"last_year_alive":focal_last_alive,"last_recorded_person_state":focal_person_state,"last_recorded_family_state":focal_last,"family_events":focal_events,"opinion_changes":focal_opinions,"recorded_meetings":focal_meetings}),
            )
        }
        Focus::Animal(animal) => {
            if focal_first.is_none() {
                return Err(format!("animal {animal} was not found in this history"));
            }
            Ok(
                json!({"scope":scope,"animal":animal,"first_observed_year":focal_first,"last_year_alive":focal_last_alive,"last_recorded_animal_state":focal_last,"recorded_population_events":focal_events,"interpretation":"Animals born in the simulation retain parent IDs; founders may lack recorded parents."}),
            )
        }
    }
}

fn main() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    let command = args
        .next()
        .ok_or("usage: inspect_history summary FILE | person FILE ID | animal FILE ID")?;
    let path = args.next().ok_or("missing history file")?;
    let focus = match command.to_str() {
        Some("summary") => Focus::Summary,
        Some("person") => Focus::Person(
            args.next()
                .ok_or("missing person id")?
                .to_string_lossy()
                .parse::<u64>()
                .map_err(|e| e.to_string())?,
        ),
        Some("animal") => Focus::Animal(
            args.next()
                .ok_or("missing animal id")?
                .to_string_lossy()
                .parse::<u64>()
                .map_err(|e| e.to_string())?,
        ),
        _ => return Err("unknown history inspection command".into()),
    };
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    let output = inspect(Path::new(&path), focus)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&output).map_err(|e| e.to_string())?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    #[test]
    fn streams_population_and_individual_history_without_inventing_status() {
        let token = format!(
            "legacy-inspect-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let path = std::env::temp_dir().join(format!("{token}.ndjson"));
        let mut file = File::create(&path).unwrap();
        let genome = json!({"sex":"Female","alleles":[[100,200],[200,400],[300,500],[400,600]]});
        let snapshot = |alive: bool| json!({"core":{"people":[{"id":1,"health":if alive {100} else {0}}]},"family":{"members":[{"id":1,"alive":alive,"genome":genome}],"events":if alive {vec![json!({"kind":"partnership","people":[1,2],"tick":1})]} else {vec![]},"totals":{"births":0,"deaths":if alive {0} else {1}}},"fauna":{"species":[{"id":1,"name":"grazer"}],"animals":[{"id":1,"species":1,"health":if alive {100} else {0},"genome":genome}]}});
        for row in [
            json!({"kind":"metadata","calendar":"annual","seed":1,"limitations":["animal parentage is not recorded by the current fauna owner"]}),
            json!({"kind":"initial","year":0,"snapshot":snapshot(true)}),
            json!({"kind":"year","year":1,"snapshot":snapshot(true),"observations":[{"kind":"meetings","events":[{"a":1,"b":2,"tick":1}]},{"kind":"opinion_changes","events":[{"observer":1,"target":2,"tick":1,"before":0,"after":1}]},{"kind":"animal_population","tick":1,"appeared":[{"id":1,"parents":[2,3]}],"inactive":[]}]}),
            json!({"kind":"year","year":2,"snapshot":snapshot(false),"observations":[]}),
            json!({"kind":"complete","years":2}),
        ] {
            writeln!(file, "{}", row).unwrap();
        }
        drop(file);
        let summary = inspect(&path, Focus::Summary).unwrap();
        assert_eq!(summary["scope"]["complete"], true);
        assert_eq!(
            summary["scope"]["metadata_corrections"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(summary["first_zero_people"], 2);
        assert_eq!(summary["first_zero_animals"], 2);
        assert_eq!(summary["animal_species_history"][0]["first_zero"], 2);
        assert_eq!(summary["recorded_meetings"], 1);
        assert_eq!(summary["genome_samples"][0]["human_locus_means"][0], 150.0);
        assert_eq!(
            summary["genome_samples"][0]["animal_species"][0]["name"],
            "grazer"
        );
        assert_eq!(
            summary["genome_samples"][0]["animal_species"][0]["locus_means"][1],
            300.0
        );
        assert_eq!(
            animal_species_samples(&[], &[json!({"id": 1, "name": "grazer"})])[0]["living"],
            0
        );
        let person = inspect(&path, Focus::Person(1)).unwrap();
        assert_eq!(person["first_observed_year"], 0);
        assert_eq!(person["last_year_alive"], 1);
        assert_eq!(person["recorded_meetings"].as_array().unwrap().len(), 1);
        assert_eq!(person["family_events"].as_array().unwrap().len(), 1);
        assert_eq!(person["opinion_changes"].as_array().unwrap().len(), 1);
        let animal = inspect(&path, Focus::Animal(1)).unwrap();
        assert_eq!(animal["last_year_alive"], 1);
        assert_eq!(
            animal["recorded_population_events"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        fs::remove_file(path).unwrap();
    }
}
