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

struct ObservationSink<'a> {
    focus: Focus,
    meetings: &'a mut BTreeMap<u64, u64>,
    meeting_count: &'a mut u64,
    focal_events: &'a mut Vec<Value>,
    focal_incapacitations: &'a mut Vec<Value>,
    focal_opinions: &'a mut Vec<Value>,
    focal_language: &'a mut Vec<Value>,
    focal_signals: &'a mut Vec<Value>,
    focal_signal_learning: &'a mut Vec<Value>,
    focal_recipe_learning: &'a mut Vec<Value>,
    focal_choices: &'a mut Vec<Value>,
    focal_mechanics: &'a mut Vec<Value>,
    focal_strikes: &'a mut Vec<Value>,
    focal_meetings: &'a mut Vec<Value>,
}
impl ObservationSink<'_> {
    fn record(&mut self, events: &[Value], year: u64) -> Result<(), String> {
        for event in events {
            if event["kind"] == "animal_population" {
                if let Focus::Animal(animal) = self.focus {
                    let appeared = event["appeared"]
                        .as_array()
                        .is_some_and(|rows| rows.iter().any(|row| id(row) == Some(animal)));
                    let inactive = event["inactive"]
                        .as_array()
                        .is_some_and(|rows| rows.iter().any(|row| row.as_u64() == Some(animal)));
                    if appeared || inactive {
                        self.focal_events.push(event.clone());
                    }
                }
                continue;
            }
            if event["kind"] == "family_events" {
                if let Focus::Person(person) = self.focus
                    && let Some(rows) = event["events"].as_array()
                {
                    self.focal_events.extend(
                        rows.iter()
                            .filter(|row| {
                                row["people"].as_array().is_some_and(|people| {
                                    people.iter().any(|id| id.as_u64() == Some(person))
                                })
                            })
                            .cloned(),
                    );
                }
                continue;
            }
            if event["kind"] == "incapacitations" {
                if let Focus::Person(person) = self.focus
                    && let Some(rows) = event["people"].as_array()
                {
                    self.focal_incapacitations.extend(
                        rows.iter()
                            .filter(|row| row["person"].as_u64() == Some(person))
                            .map(|row| {
                                json!({"at_millis":event["at_millis"],
                            "died":row["died"], "natural_mortality":row["natural_mortality"]})
                            }),
                    );
                }
                continue;
            }
            if event["kind"] == "opinion_changes" {
                if let Focus::Person(person) = self.focus
                    && let Some(rows) = event["events"].as_array()
                {
                    self.focal_opinions.extend(
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
            if event["kind"] == "language_events" {
                if let Focus::Person(person) = self.focus
                    && let Some(rows) = event["events"].as_array()
                {
                    self.focal_language.extend(
                        rows.iter()
                            .filter(|row| {
                                row["speaker"].as_u64() == Some(person)
                                    || row["listener"].as_u64() == Some(person)
                            })
                            .cloned(),
                    );
                }
                continue;
            }
            if event["kind"] == "signal_events" {
                if let Focus::Person(person) = self.focus
                    && let Some(rows) = event["events"].as_array()
                {
                    self.focal_signals.extend(
                        rows.iter()
                            .filter(|row| {
                                row["actor"].as_u64() == Some(person)
                                    || row["observer"].as_u64() == Some(person)
                            })
                            .cloned(),
                    );
                }
                continue;
            }
            if event["kind"] == "signal_learning" {
                if let Focus::Person(person) = self.focus
                    && let Some(rows) = event["events"].as_array()
                {
                    self.focal_signal_learning.extend(
                        rows.iter()
                            .filter(|row| {
                                row["actor"].as_u64() == Some(person)
                                    || row["observer"].as_u64() == Some(person)
                            })
                            .cloned(),
                    );
                }
                continue;
            }
            if event["kind"] == "recipe_learning" {
                if let Focus::Person(person) = self.focus
                    && let Some(rows) = event["events"].as_array()
                {
                    self.focal_recipe_learning.extend(
                        rows.iter()
                            .filter(|row| {
                                row["teacher"].as_u64() == Some(person)
                                    || row["student"].as_u64() == Some(person)
                            })
                            .cloned(),
                    );
                }
                continue;
            }
            if event["kind"] == "personal_choices" {
                if let Focus::Person(person) = self.focus
                    && let Some(rows) = event["events"].as_array()
                {
                    self.focal_choices.extend(
                        rows.iter()
                            .filter(|row| row["actor"].as_u64() == Some(person))
                            .cloned(),
                    );
                }
                continue;
            }
            if event["kind"] == "mechanical_experiments" {
                if let Focus::Person(person) = self.focus
                    && let Some(rows) = event["events"].as_array()
                {
                    self.focal_mechanics.extend(
                        rows.iter()
                            .filter(|row| row["actor"].as_u64() == Some(person))
                            .cloned(),
                    );
                }
                continue;
            }
            if event["kind"] == "personal_strikes" {
                if let Focus::Person(person) = self.focus
                    && let Some(rows) = event["events"].as_array()
                {
                    let strikes: Vec<_> = rows
                        .iter()
                        .filter(|row| {
                            row["actor"].as_u64() == Some(person)
                                || row["target"].as_u64() == Some(person)
                        })
                        .cloned()
                        .collect();
                    if !strikes.is_empty() {
                        let harm: Vec<_> = event["harm"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter(|row| row["target"].as_u64() == Some(person))
                            .cloned()
                            .collect();
                        self.focal_strikes.push(
                                            json!({"year":year,"tick":event.get("tick"),"events":strikes,"harm":harm}),
                                        );
                    }
                }
                continue;
            }
            if event["kind"] != "meetings" {
                continue;
            }
            if let Some(encounters) = event["events"].as_array() {
                for encounter in encounters {
                    let (Some(a), Some(b)) = (encounter["a"].as_u64(), encounter["b"].as_u64())
                    else {
                        return Err("invalid meeting identity".into());
                    };
                    *self.meetings.entry(a).or_default() += 1;
                    *self.meetings.entry(b).or_default() += 1;
                    *self.meeting_count += 1;
                    if matches!(self.focus,Focus::Person(person) if person==a||person==b) {
                        self.focal_meetings.push(encounter.clone());
                    }
                }
            }
        }
        Ok(())
    }
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
    let mut focal_incapacitations = Vec::new();
    let mut focal_opinions = Vec::new();
    let mut focal_language = Vec::new();
    let mut focal_signals = Vec::new();
    let mut focal_signal_learning = Vec::new();
    let mut focal_recipe_learning = Vec::new();
    let mut focal_choices = Vec::new();
    let mut focal_mechanics = Vec::new();
    let mut focal_strikes = Vec::new();
    let mut previous_year = None;
    let mut pending_year = None;
    let mut batch_has_family_events = false;
    macro_rules! record_observations {
        ($events:expr, $year:expr) => {{
            let mut sink = ObservationSink {
                focus,
                meetings: &mut meetings,
                meeting_count: &mut meeting_count,
                focal_events: &mut focal_events,
                focal_incapacitations: &mut focal_incapacitations,
                focal_opinions: &mut focal_opinions,
                focal_language: &mut focal_language,
                focal_signals: &mut focal_signals,
                focal_signal_learning: &mut focal_signal_learning,
                focal_recipe_learning: &mut focal_recipe_learning,
                focal_choices: &mut focal_choices,
                focal_mechanics: &mut focal_mechanics,
                focal_strikes: &mut focal_strikes,
                focal_meetings: &mut focal_meetings,
            };
            sink.record($events, $year)?;
        }};
    }
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
            Some("observation_batch") => {
                let year = row["year"]
                    .as_u64()
                    .ok_or("observation batch has no year")?;
                if previous_year.is_none_or(|previous: u64| previous.checked_add(1) != Some(year)) {
                    return Err("observation batch is outside its annual interval".into());
                }
                if pending_year.is_some_and(|previous| previous != year) {
                    return Err("observation batches cross annual snapshots".into());
                }
                pending_year = Some(year);
                let events = row["observations"]
                    .as_array()
                    .ok_or("invalid observation batch")?;
                batch_has_family_events |=
                    events.iter().any(|event| event["kind"] == "family_events");
                record_observations!(events, year);
            }
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
                if pending_year.is_some() {
                    return Err("history ends before its annual snapshot".into());
                }
                complete = true;
                completed_years = row["years"].as_u64();
            }
            Some("initial" | "year") => {
                let year = row
                    .get("year")
                    .and_then(Value::as_u64)
                    .ok_or("history row has no year")?;
                if row["kind"] == "year"
                    && let Some(batch_year) = pending_year.take()
                    && batch_year != year
                {
                    return Err("observation batch has the wrong year".into());
                }
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
                        record_observations!(events, year);
                    }
                    if !batch_has_family_events
                        && !row["observations"].as_array().is_some_and(|events| {
                            events.iter().any(|event| event["kind"] == "family_events")
                        })
                        && let Focus::Person(person) = focus
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
                    batch_has_family_events = false;
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
                json!({"scope":scope,"person":person,"first_observed_year":focal_first,"last_year_alive":focal_last_alive,"last_recorded_person_state":focal_person_state,"last_recorded_family_state":focal_last,"family_events":focal_events,"incapacitations":focal_incapacitations,"opinion_changes":focal_opinions,"language_events":focal_language,"signal_events":focal_signals,"signal_learning":focal_signal_learning,"recipe_learning":focal_recipe_learning,"personal_choices":focal_choices,"mechanical_experiments":focal_mechanics,"personal_strikes":focal_strikes,"recorded_meetings":focal_meetings}),
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

fn narrative(report: &Value, focus: Focus) -> String {
    use std::fmt::Write;

    let scope = &report["scope"];
    let count = |field: &str| report[field].as_array().map_or(0, Vec::len);
    let mut text = String::new();
    match focus {
        Focus::Summary => {
            text.push_str("# World history\n\n");
            let people = &report["peak_people"];
            let animals = &report["peak_animals"];
            writeln!(text, "The largest recorded human population was {} in year {}. The largest recorded animal population was {} in year {}.\n", people["count"], people["year"], animals["count"], animals["year"]).unwrap();
            if let Some(year) = report["first_zero_people"].as_u64() {
                writeln!(
                    text,
                    "Year {year} is the first snapshot with nobody still alive.\n"
                )
                .unwrap();
            }
            let totals = &report["family_totals"];
            if let (Some(births), Some(deaths)) =
                (totals["births"].as_u64(), totals["deaths"].as_u64())
            {
                writeln!(text, "The final family counters record {births} births and {deaths} deaths. The history contains {} observed meetings.\n", report["recorded_meetings"]).unwrap();
            }
            for species in report["animal_species_history"]
                .as_array()
                .into_iter()
                .flatten()
            {
                let name = species["name"].as_str().unwrap_or("Unnamed species");
                write!(
                    text,
                    "{name} peaked at {} in year {}.",
                    species["peak"]["count"], species["peak"]["year"]
                )
                .unwrap();
                if let Some(year) = species["first_zero"].as_u64() {
                    write!(
                        text,
                        " Its first recorded year without a living animal was {year}."
                    )
                    .unwrap();
                }
                text.push_str("\n\n");
            }
        }
        Focus::Person(person) => {
            writeln!(text, "# Person {person}\n").unwrap();
            observed_life(&mut text, report);
            let incapacitations = report["incapacitations"]
                .as_array()
                .map_or(&[][..], Vec::as_slice);
            let exact_death = incapacitations
                .iter()
                .any(|event| event["died"].as_bool() == Some(true));
            for event in report["family_events"].as_array().into_iter().flatten() {
                let Some(tick) = event["tick"].as_u64() else {
                    continue;
                };
                let related: Vec<_> = event["people"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_u64)
                    .collect();
                let other = related.iter().copied().find(|id| *id != person);
                let sentence = match (event["kind"].as_str(), related.as_slice()) {
                    (Some("birth"), [child, mother, father]) if *child == person => {
                        format!("Person {person} was born to people {mother} and {father}.")
                    }
                    (Some("birth"), [child, _, _]) => {
                        format!("their child, person {child}, was born.")
                    }
                    (Some("adulthood"), _) => "they reached adulthood.".to_owned(),
                    (Some("partnership"), _) if other.is_some() => {
                        format!("they formed a partnership with person {}.", other.unwrap())
                    }
                    (Some("separation"), _) if other.is_some() => {
                        format!("their partnership with person {} ended.", other.unwrap())
                    }
                    (Some("conception"), _) if other.is_some() => {
                        format!("they and person {} conceived a child.", other.unwrap())
                    }
                    (Some("pregnancy_loss"), _) => {
                        "a pregnancy involving them ended in loss.".to_owned()
                    }
                    (Some("death"), _) if !exact_death => "they died.".to_owned(),
                    _ => continue,
                };
                writeln!(text, "At tick {tick}, {sentence}\n").unwrap();
            }
            for event in incapacitations {
                let Some(time) = event["at_millis"].as_u64() else {
                    continue;
                };
                let day = time / 86_400_000;
                let hour = time / 3_600_000 % 24;
                let minute = time / 60_000 % 60;
                let second = time / 1000 % 60;
                let millis = time % 1000;
                let result = match (
                    event["died"].as_bool(),
                    event["natural_mortality"].as_bool(),
                ) {
                    (Some(true), Some(true)) => "died of natural causes",
                    (Some(true), _) => "died",
                    _ => "became incapacitated",
                };
                writeln!(text, "At {hour:02}:{minute:02}:{second:02}.{millis:03} on simulated day {day}, they {result}.\n").unwrap();
            }
            writeln!(text, "The recorded activity includes {} meetings, {} spoken exchanges, {} instances of recipe learning or teaching, and {} mechanical experiments involving them.\n",
                count("recorded_meetings"), count("language_events"), count("recipe_learning"), count("mechanical_experiments")).unwrap();
            for experiment in report["mechanical_experiments"]
                .as_array()
                .into_iter()
                .flatten()
                .take(5)
            {
                if let (Some(tick), Some(outcome)) =
                    (experiment["tick"].as_u64(), experiment["outcome"].as_str())
                {
                    writeln!(text, "At tick {tick}, a mechanical experiment involving them recorded the outcome `{outcome}`.\n").unwrap();
                }
            }
        }
        Focus::Animal(animal) => {
            writeln!(text, "# Animal {animal}\n").unwrap();
            observed_life(&mut text, report);
            let state = &report["last_recorded_animal_state"];
            if let Some(species) = state["species"].as_u64() {
                writeln!(
                    text,
                    "The last recorded state assigns them to species {species}.\n"
                )
                .unwrap();
            }
            if let Some(parents) = state["parents"]
                .as_array()
                .filter(|parents| parents.len() == 2)
            {
                writeln!(
                    text,
                    "Their recorded parents are animals {} and {}.\n",
                    parents[0], parents[1]
                )
                .unwrap();
            }
        }
    }
    if let (Some(first), Some(last)) = (scope["first_year"].as_u64(), scope["last_year"].as_u64()) {
        writeln!(
            text,
            "This account covers recorded years {first} through {last}."
        )
        .unwrap();
    }
    text
}

fn observed_life(text: &mut String, report: &Value) {
    use std::fmt::Write;
    if let Some(first) = report["first_observed_year"].as_u64() {
        writeln!(text, "They first appear in the year {first} snapshot.\n").unwrap();
    }
    if let Some(last) = report["last_year_alive"].as_u64() {
        writeln!(
            text,
            "The last snapshot that records them alive is from year {last}.\n"
        )
        .unwrap();
    }
}

fn main() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    let command = args.next().ok_or(
        "usage: inspect_history (summary FILE | person FILE ID | animal FILE ID) [--narrative]",
    )?;
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
    let narrative_requested = match args.next() {
        None => false,
        Some(flag) if flag == "--narrative" && args.next().is_none() => true,
        Some(_) => return Err("unexpected argument".into()),
    };
    let output = inspect(Path::new(&path), focus)?;
    if narrative_requested {
        print!("{}", narrative(&output, focus));
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&output).map_err(|e| e.to_string())?
        );
    }
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
        for mut row in [
            json!({"kind":"metadata","calendar":"annual","seed":1,"limitations":["animal parentage is not recorded by the current fauna owner"]}),
            json!({"kind":"initial","year":0,"snapshot":snapshot(true)}),
            json!({"kind":"observation_batch","year":1,"observations":[{"kind":"mechanical_experiments","events":[{"actor":1,"outcome":"stalled","tick":1}]}]}),
            json!({"kind":"year","year":1,"snapshot":snapshot(true),"observations":[{"kind":"meetings","events":[{"a":1,"b":2,"tick":1}]},{"kind":"opinion_changes","events":[{"observer":1,"target":2,"tick":1,"before":0,"after":1}]},{"kind":"language_events","events":[{"speaker":1,"listener":2,"tick":1,"sounds":[0]}]},{"kind":"signal_events","events":[{"actor":2,"observer":1,"tick":1,"form":1,"noticed":true,"interpreted":2}]},{"kind":"signal_learning","events":[{"actor":2,"observer":1,"tick":1,"form":1,"good":4,"meaning":2}]},{"kind":"personal_choices","events":[{"actor":1,"outcome":"accepted","tick":1}]},{"kind":"mechanical_experiments","events":[{"actor":1,"outcome":"moved","tick":1}]},{"kind":"animal_population","tick":1,"appeared":[{"id":1,"parents":[2,3]}],"inactive":[]}]}),
            json!({"kind":"year","year":2,"snapshot":snapshot(false),"observations":[
            {"kind":"incapacitations","at_millis":40_000_000_000u64,"people":[
                {"person":1,"died":true,"natural_mortality":true},
                {"person":2,"died":false,"natural_mortality":false}
            ]}]}),
            json!({"kind":"complete","years":2}),
        ] {
            if row["kind"] == "initial" {
                row["snapshot"]["core"]["people"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":2,"health":0}));
            }
            if row["kind"] == "year" && row["year"] == 1 {
                row["observations"].as_array_mut().unwrap().push(json!({
                    "kind":"recipe_learning", "events":[{"teacher":1,"student":2,"recipe":3,"tick":1}]
                }));
                row["observations"].as_array_mut().unwrap().push(json!({
                    "kind":"personal_strikes",
                    "events":[{"actor":1,"target":2,"force":8,"defended":2,"harm":6},
                        {"actor":3,"target":2,"force":5,"defended":2,"harm":3}],
                    "harm":[{"target":2,"before":10,"after":1}]
                }));
            }
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
        let life = narrative(&person, Focus::Person(1));
        assert!(life.contains("formed a partnership with person 2"));
        assert!(life.contains("died of natural causes"));
        assert!(life.contains("recorded the outcome `moved`"));
        assert!(!life.contains("became incapacitated"));
        let overview = narrative(&summary, Focus::Summary);
        assert!(overview.contains("Year 2 is the first snapshot with nobody still alive"));
        assert!(overview.contains("grazer peaked at 1 in year 0"));
        assert_eq!(person["first_observed_year"], 0);
        assert_eq!(person["last_year_alive"], 1);
        assert_eq!(person["recorded_meetings"].as_array().unwrap().len(), 1);
        assert_eq!(person["family_events"].as_array().unwrap().len(), 1);
        assert_eq!(
            person["incapacitations"],
            json!([
                {"at_millis":40_000_000_000u64,"died":true,"natural_mortality":true}
            ])
        );
        assert_eq!(person["opinion_changes"].as_array().unwrap().len(), 1);
        assert_eq!(person["language_events"].as_array().unwrap().len(), 1);
        assert_eq!(person["signal_events"].as_array().unwrap().len(), 1);
        assert_eq!(person["signal_learning"].as_array().unwrap().len(), 1);
        assert_eq!(person["recipe_learning"].as_array().unwrap().len(), 1);
        assert_eq!(person["personal_choices"].as_array().unwrap().len(), 1);
        assert_eq!(person["personal_strikes"][0]["year"], 1);
        assert_eq!(
            person["personal_strikes"][0]["events"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(
            person["personal_strikes"][0]["harm"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let target = inspect(&path, Focus::Person(2)).unwrap();
        assert_eq!(
            target["personal_strikes"][0]["events"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(target["personal_strikes"][0]["harm"][0]["after"], 1);
        assert_eq!(
            person["mechanical_experiments"].as_array().unwrap().len(),
            2
        );
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
