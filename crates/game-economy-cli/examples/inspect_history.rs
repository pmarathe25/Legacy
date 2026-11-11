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

fn inspect(path: &Path, focus: Focus) -> Result<Value, String> {
    let input = BufReader::new(File::open(path).map_err(|e| e.to_string())?);
    let mut metadata = Value::Null;
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
                expected_years = row["years_requested"].as_u64();
                metadata = json!({"calendar":row["calendar"],"seed":row["seed"],"git_revision":row["git_revision"],"source_kind":row["source_kind"],"limitations":row["limitations"]});
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
                    samples.push(json!({"year":year,"living_people":live_people,"living_animals":live_animals,"human_locus_means":human_means,"animal_locus_means":animal_means}));
                }
                if row["kind"] == "year" {
                    if let Some(events) = row["observations"].as_array() {
                        for event in events {
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
                    if let Focus::Person(person) = focus
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
    let scope = json!({"source":path,"complete":complete,"first_year":first_year,"last_year":last_year,"annual_records":years,"metadata":metadata});
    match focus {
        Focus::Summary => Ok(
            json!({"scope":scope,"peak_people":{"year":peak_people.0,"count":peak_people.1},"peak_animals":{"year":peak_animals.0,"count":peak_animals.1},"first_zero_people":human_zero,"first_zero_animals":animal_zero,"family_totals":totals,"recorded_meetings":meeting_count,"most_recorded_meetings":ranking.iter().map(|(person,count)|json!({"person":person,"count":count})).collect::<Vec<_>>(),"genome_samples":samples,"interpretation":"Meeting counts do not establish leadership; changes in allele means do not establish selection or causation."}),
        ),
        Focus::Person(person) => {
            if focal_first.is_none() {
                return Err(format!("person {person} was not found in this history"));
            }
            Ok(
                json!({"scope":scope,"person":person,"first_observed_year":focal_first,"last_year_alive":focal_last_alive,"last_recorded_person_state":focal_person_state,"last_recorded_family_state":focal_last,"family_events":focal_events,"recorded_meetings":focal_meetings}),
            )
        }
        Focus::Animal(animal) => {
            if focal_first.is_none() {
                return Err(format!("animal {animal} was not found in this history"));
            }
            Ok(
                json!({"scope":scope,"animal":animal,"first_observed_year":focal_first,"last_year_alive":focal_last_alive,"last_recorded_animal_state":focal_last,"interpretation":"The current fauna owner does not retain animal parentage."}),
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
        let snapshot = |alive: bool| json!({"core":{"people":[{"id":1,"health":if alive {100} else {0}}]},"family":{"members":[{"id":1,"alive":alive,"genome":genome}],"events":if alive {vec![json!({"kind":"partnership","people":[1,2],"tick":1})]} else {vec![]},"totals":{"births":0,"deaths":if alive {0} else {1}}},"fauna":{"animals":[{"id":1,"health":if alive {100} else {0},"genome":genome}]}});
        for row in [
            json!({"kind":"metadata","calendar":"annual","seed":1,"limitations":[]}),
            json!({"kind":"initial","year":0,"snapshot":snapshot(true)}),
            json!({"kind":"year","year":1,"snapshot":snapshot(true),"observations":[{"kind":"meetings","events":[{"a":1,"b":2,"tick":1}]}]}),
            json!({"kind":"year","year":2,"snapshot":snapshot(false),"observations":[]}),
            json!({"kind":"complete","years":2}),
        ] {
            writeln!(file, "{}", row).unwrap();
        }
        drop(file);
        let summary = inspect(&path, Focus::Summary).unwrap();
        assert_eq!(summary["scope"]["complete"], true);
        assert_eq!(summary["first_zero_people"], 2);
        assert_eq!(summary["first_zero_animals"], 2);
        assert_eq!(summary["recorded_meetings"], 1);
        assert_eq!(summary["genome_samples"][0]["human_locus_means"][0], 150.0);
        let person = inspect(&path, Focus::Person(1)).unwrap();
        assert_eq!(person["first_observed_year"], 0);
        assert_eq!(person["last_year_alive"], 1);
        assert_eq!(person["recorded_meetings"].as_array().unwrap().len(), 1);
        assert_eq!(person["family_events"].as_array().unwrap().len(), 1);
        let animal = inspect(&path, Focus::Animal(1)).unwrap();
        assert_eq!(animal["last_year_alive"], 1);
        fs::remove_file(path).unwrap();
    }
}
