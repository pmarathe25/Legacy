//! Summaries of attributed actions in the archive, outside simulation state.
#[path = "chronicle/crew_turnover.rs"]
mod crew_turnover;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct SupplyGroup {
    recipient: u64,
    suppliers: Vec<u64>,
}

struct SupplyEpisode {
    first: u64,
    last: u64,
}

struct ExpeditionParticipation {
    first_year: u64,
    latest_year: u64,
    people: BTreeSet<u64>,
    joined: u64,
    joined_under_pressure: u64,
}

#[derive(Default)]
struct CrewParticipation {
    helpers: BTreeSet<u64>,
    starts: u64,
    years: BTreeMap<u64, CrewYear>,
}

#[derive(Default)]
struct CrewYear {
    helpers: BTreeMap<u64, u64>,
    starts: u64,
}

impl CrewParticipation {
    fn record(&mut self, year: u64, helper: u64) {
        self.starts = self.starts.saturating_add(1);
        self.helpers.insert(helper);
        let annual = self.years.entry(year).or_default();
        annual.starts = annual.starts.saturating_add(1);
        let starts = annual.helpers.entry(helper).or_default();
        *starts = starts.saturating_add(1);
    }

    fn summary(&self, lead: u64) -> Value {
        let peak = self
            .years
            .iter()
            .max_by_key(|(year, row)| (row.starts, row.helpers.len(), std::cmp::Reverse(**year)))
            .map(|(year, row)| {
                json!({
                    "year": year, "starts": row.starts, "helpers": row.helpers.len()
                })
            });
        json!({
            "lead": lead, "helpers": self.helpers, "starts": self.starts,
            "first_year": self.years.first_key_value().map(|(year, _)| year),
            "last_year": self.years.last_key_value().map(|(year, _)| year),
            "peak": peak,
            "years": self.years.iter().map(|(year, row)| json!({
                "year": year, "starts": row.starts, "helpers": row.helpers.keys().collect::<Vec<_>>()
            })).collect::<Vec<_>>(),
        })
    }
}

struct RecipeTransmission {
    first_year: u64,
    last_year: u64,
    teachers: BTreeSet<u64>,
    learners: BTreeSet<u64>,
}

#[derive(Default)]
pub(super) struct Chronicle {
    recipes: BTreeMap<u64, String>,
    lexicon: super::lexicon::LexiconHistory,
    coordination: super::coordination::History,
    inherited_traits: super::named_traits::History,
    recipe_learning: BTreeMap<u64, RecipeTransmission>,
    crews: BTreeMap<u64, CrewParticipation>,
    counts: BTreeMap<&'static str, u64>,
    people: BTreeMap<u64, BTreeMap<&'static str, u64>>,
    selected: Vec<Value>,
    previous_delivered: BTreeMap<u64, u64>,
    active_ties: BTreeMap<(u64, u64), u64>,
    repeated_ties: BTreeMap<(u64, u64), (u64, u64, u64)>,
    supply_episodes: BTreeMap<SupplyGroup, Vec<SupplyEpisode>>,
    expedition_participation: BTreeMap<u64, ExpeditionParticipation>,
    mechanical_findings: std::collections::VecDeque<Value>,
}

#[derive(Clone, Copy)]
struct Receipt {
    label: &'static str,
    actor: Option<u64>,
    other: Option<u64>,
}

impl Chronicle {
    pub(super) fn snapshot(&mut self, state: &Value, year: u64, opening: bool) {
        self.lexicon.observe(state, year);
        self.inherited_traits.observe(state, year);
        if opening {
            self.recipes = state
                .pointer("/core/physical/recipes")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|row| Some((row["id"].as_u64()?, row["name"].as_str()?.to_owned())))
                .collect();
        }
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
        let mut suppliers: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
        for &pair in now_active.keys() {
            self.coordination.observe(pair.1, pair.0, year, super::coordination::Evidence::Delivery);
            if let Some(&earlier) = self.active_ties.get(&pair)
                && earlier.checked_add(1) == Some(year)
            {
                let entry = self
                    .repeated_ties
                    .entry(pair)
                    .or_insert((earlier, earlier, 1));
                entry.2 += 1 + u64::from(entry.1 != earlier);
                entry.1 = year;
                suppliers.entry(pair.1).or_default().push(pair.0);
            }
        }
        for (recipient, people) in suppliers.into_iter().filter(|(_, people)| people.len() > 1) {
            let episodes = self
                .supply_episodes
                .entry(SupplyGroup {
                    recipient,
                    suppliers: people,
                })
                .or_default();
            let first = year.saturating_sub(1);
            if let Some(last) = episodes.last_mut().filter(|row| row.last == first) {
                last.last = year;
            } else {
                episodes.push(SupplyEpisode { first, last: year });
            }
        }
        self.previous_delivered = observed;
        self.active_ties = now_active;
    }

    pub(super) fn observe(&mut self, event: &Value, year: u64, focus: Option<u64>) {
        let Some(kind) = event["kind"].as_str() else {
            return;
        };
        if kind == "elapsed_fauna_events" {
            let count = event["migrations"].as_array().map_or(0, |rows| rows.len()) as u64;
            if count != 0 {
                *self
                    .counts
                    .entry("wildlife corridor crossings")
                    .or_default() += count;
            }
            return;
        }
        if kind == "crew_starts" {
            for row in event["events"].as_array().into_iter().flatten() {
                let (Some(helper), Some(lead)) = (row["helper"].as_u64(), row["lead"].as_u64())
                else {
                    continue;
                };
                if helper == 0 || lead == 0 || helper == lead {
                    continue;
                }
                self.crews.entry(lead).or_default().record(year, helper);
                self.coordination.observe(lead, helper, year, super::coordination::Evidence::Crew);
                self.record(
                    event,
                    row,
                    year,
                    focus,
                    Receipt {
                        label: "decisions to join another person's work",
                        actor: Some(helper),
                        other: Some(lead),
                    },
                );
            }
            return;
        }
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
                | "care"
                | "expeditions"
                | "tactical"
                | "confinement"
                | "mechanical_experiments"
                | "mechanical_conclusions"
                | "mechanical_assemblies"
                | "soil_amendments"
                | "pastimes"
                | "heard_performances"
                | "work_requests"
                | "maturation"
                | "recipe_learning"
                | "language_events"
                | "written_explanations"
                | "written_works"
                | "farm_review"
                | "personal_practices"
                | "agreement_expiry"
                | "promise_transfers"
        ) {
            return;
        }
        for row in event["events"].as_array().into_iter().flatten() {
            if kind == "mechanical_conclusions"
                && focus.is_none_or(|person| row["person"].as_u64() == Some(person))
                && classify(kind, row).is_some()
            {
                if self.mechanical_findings.len() == 12 {
                    self.mechanical_findings.pop_front();
                }
                self.mechanical_findings.push_back(json!({ "year": year, "person": row["person"],
                    "at_millis": row["at_millis"], "evidence": row["evidence"] }));
            }
            if kind == "expeditions" {
                self.record_expedition_participation(row, year);
            }
            if kind == "recipe_learning"
                && let (Some(recipe), Some(teacher), Some(student)) = (
                    row["recipe"].as_u64(),
                    row["teacher"].as_u64(),
                    row["student"].as_u64(),
                )
            {
                let learning =
                    self.recipe_learning
                        .entry(recipe)
                        .or_insert_with(|| RecipeTransmission {
                            first_year: year,
                            last_year: year,
                            teachers: BTreeSet::new(),
                            learners: BTreeSet::new(),
                        });
                learning.first_year = learning.first_year.min(year);
                learning.last_year = learning.last_year.max(year);
                learning.teachers.insert(teacher);
                learning.learners.insert(student);
            }
            if let Some(receipt) = classify(kind, row) {
                self.record(event, row, year, focus, receipt);
            }
        }
    }

    fn record_expedition_participation(&mut self, row: &Value, year: u64) {
        let invitation = &row["kind"]["invitation"];
        let (Some(inviter), Some(recipient), Some(response)) = (
            row["person"].as_u64(),
            invitation["recipient"].as_u64(),
            invitation["response"].as_str(),
        ) else {
            return;
        };
        if !matches!(response, "joined" | "joined_under_pressure") {
            return;
        }
        let entry = self
            .expedition_participation
            .entry(inviter)
            .or_insert_with(|| ExpeditionParticipation {
                first_year: year,
                latest_year: year,
                people: BTreeSet::new(),
                joined: 0,
                joined_under_pressure: 0,
            });
        entry.latest_year = year;
        entry.people.insert(recipient);
        if response == "joined" {
            entry.joined += 1;
        } else {
            entry.joined_under_pressure += 1;
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
        let mut supply: Vec<_> = self
            .supply_episodes
            .iter()
            .flat_map(|(people, episodes)| episodes.iter().map(move |years| (people, years)))
            .collect();
        supply.sort_unstable_by_key(|&(group, years)| {
            (
                std::cmp::Reverse(years.last - years.first),
                group,
                years.first,
            )
        });
        supply.truncate(10);
        let mut expeditions: Vec<_> = self.expedition_participation.iter().collect();
        expeditions
            .sort_unstable_by_key(|&(person, row)| (std::cmp::Reverse(row.people.len()), *person));
        expeditions.truncate(10);
        let mut transmissions: Vec<_> = self.recipe_learning.iter().collect();
        transmissions.sort_unstable_by_key(|&(recipe, learning)| {
            (std::cmp::Reverse(learning.learners.len()), *recipe)
        });
        transmissions.truncate(10);
        let mut crews: Vec<_> = self.crews.iter().collect();
        crews.sort_unstable_by_key(|&(lead, crew)| {
            (
                std::cmp::Reverse(crew.helpers.len()),
                std::cmp::Reverse(crew.starts),
                *lead,
            )
        });
        crews.truncate(10);
        json!({
            "shared_crews": crews.iter().map(|&(&lead, crew)| crew.summary(lead)).collect::<Vec<_>>(),
            "crew_turnover": crew_turnover::transitions(&self.crews),
            "counts": self.counts,
            "coordination_patterns": self.coordination.summary(),
            "recent_mechanical_findings": self.mechanical_findings,
            "lexicon": self.lexicon.summary(),
            "named_inherited_traits": self.inherited_traits.summary(),
            "recipe_transmission": transmissions.iter().map(|&(&id, learning)| json!({
                "recipe": id, "name": self.recipes.get(&id),
                "first_year": learning.first_year, "last_year": learning.last_year,
                "teachers": learning.teachers, "learners": learning.learners
            })).collect::<Vec<_>>(),
            "people": contributors.iter().map(|&(person, counts)| json!({"person":person,"counts":counts})).collect::<Vec<_>>(),
            "recurring_deliveries": ties.iter().map(|&(&(sender, recipient), &(first, latest, years))| {
                json!({"sender":sender,"recipient":recipient,"first_year":first,"latest_year":latest,"recorded_years":years})
            }).collect::<Vec<_>>(),
            "recurring_supply": supply.iter().map(|&(group, years)| {
                json!({"recipient":group.recipient,"suppliers":group.suppliers,"first_year":years.first,"latest_year":years.last})
            }).collect::<Vec<_>>(),
            "expedition_participation": expeditions.iter().map(|&(&person, row)| {
                json!({"inviter":person,"participants":row.people,"joined":row.joined,"joined_under_pressure":row.joined_under_pressure,"first_year":row.first_year,"latest_year":row.latest_year})
            }).collect::<Vec<_>>()
        })
    }

    pub(super) fn selected(self) -> Vec<Value> {
        self.selected
    }
}

fn classify(kind: &str, row: &Value) -> Option<Receipt> {
    if kind == "language_events" {
        let marker = row.pointer("/clause/role_marker")?.as_object()?;
        let adopted = marker.get("adopted").is_some_and(Value::is_object);
        return Some(Receipt {
            label: if adopted { "personal role particles adopted" } else { "role-marked phrases spoken" },
            actor: row[if adopted { "listener" } else { "speaker" }].as_u64(),
            other: row[if adopted { "speaker" } else { "listener" }].as_u64(),
        });
    }
    if kind == "mechanical_conclusions" {
        let evidence = &row["evidence"];
        let moved = evidence["largest_load_moved"].as_u64()?;
        let stalled = evidence["smallest_load_stalled"].as_u64()?;
        return Some(Receipt {
            label: if moved < stalled { "conditional load estimates recorded" } else { "contradictory mechanical comparisons recorded" },
            actor: Some(row["person"].as_u64()?),
            other: None,
        });
    }
    if kind == "promise_transfers" {
        return Some(Receipt {
            label: "delivered promises",
            actor: Some(row["debtor"].as_u64()?),
            other: Some(row["creditor"].as_u64()?),
        });
    }
    if kind == "written_works" {
        return Some(Receipt { label: "written works composed", actor: Some(row["person"].as_u64()?), other: None });
    }
    if kind == "heard_performances" {
        return Some(Receipt {
            label: "performances heard at their conclusion",
            actor: Some(row["listener"].as_u64()?),
            other: Some(row["performer"].as_u64()?),
        });
    }
    if kind == "care" {
        let child = row["child"].as_u64()?;
        let caregiver = row["caregiver"].as_u64();
        return Some(Receipt {
            label: if caregiver.is_some() {
                "care visits"
            } else {
                "missed care visits"
            },
            actor: caregiver.or(Some(child)),
            other: caregiver.map(|_| child),
        });
    }
    if kind == "recipe_learning" {
        return Some(Receipt {
            label: "recipe lessons recorded",
            actor: Some(row["teacher"].as_u64()?),
            other: Some(row["student"].as_u64()?),
        });
    }
    if kind == "written_explanations" {
        let speaker = row["speaker"].as_u64()?;
        let listener = row["listener"].as_u64()?;
        let adoption = row["adopted"].as_object();
        let (label, actor, other) = match adoption {
            Some(_) if row["adopted"]["marks"] != row["marks"] => (
                "written forms adapted after an explanation",
                listener,
                speaker,
            ),
            Some(_) => (
                "written forms adopted after an explanation",
                listener,
                speaker,
            ),
            None => (
                "written explanations without recorded adoption",
                speaker,
                listener,
            ),
        };
        return Some(Receipt {
            label,
            actor: Some(actor),
            other: Some(other),
        });
    }
    if kind == "maturation" {
        let label = match row["outcome"].as_str()? {
            "matured" => "prepared batches matured",
            "unsuitable_temperature" => "prepared batches lost at unsuitable temperatures",
            "storage_unavailable" => "prepared batches lost without suitable storage",
            _ => return None,
        };
        let place = &row["place"];
        let holder = (place["kind"] == "person")
            .then(|| place["id"].as_u64())
            .flatten();
        return Some(Receipt {
            label,
            actor: None,
            other: holder,
        });
    }
    let person = row["person"].as_u64();
    let kind_value = &row["kind"];
    let category = kind_value
        .as_str()
        .or_else(|| {
            let fields = kind_value.as_object()?;
            (fields.len() == 1).then(|| fields.keys().next().expect("one variant").as_str())
        })
        .unwrap_or_default();
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
        ("expeditions", "departure_scheduled") => "expedition departure plans revised",
        ("expeditions", "rendezvous_changed") => "individual expedition rendezvous plans revised",
        ("expeditions", "companions_departed") => "expedition participants who departed together",
        ("expeditions", "supplies_acquired") => "expedition supplies acquired",
        ("expeditions", "invitation") => match kind_value["invitation"]["response"].as_str() {
            None => "expedition invitations without an immediate reply",
            Some("joined") => "expedition invitations accepted",
            Some("joined_under_pressure") => "expedition invitations accepted under pressure",
            Some("declined") => "expedition invitations declined",
            Some("withdrawn") => "expedition invitations withdrawn",
            _ => return None,
        },
        ("expeditions", "objective_met") => "completed expedition objectives",
        ("expeditions", "reached_origin") => "expedition returns",
        ("expeditions", "abandoned") => "abandoned expeditions",
        ("tactical", "started") => "tactical encounters started",
        ("tactical", "entered") => "tactical interventions",
        ("tactical", "withdrew") => "tactical withdrawals",
        ("confinement", "confined") => "captures",
        ("confinement", "released") => "releases from holds",
        ("mechanical_assemblies", _) if row["accepted"].as_bool() == Some(true) => {
            "mechanical apparatus assembled"
        }
        ("mechanical_experiments", _) => match row["outcome"].as_str()? {
            "moved" => "mechanical tests that moved a load",
            "stalled" => "mechanical tests that stalled",
            "broke" => "mechanical tests that broke a part",
            _ => return None,
        },
        ("soil_amendments", _) if row["accepted"].as_bool() == Some(true) => {
            "soil amendments completed"
        }
        ("farm_review", _) => match row["resulting_phase"].as_str()? {
            "growing" => "fields planted",
            "fallow" => "fields harvested",
            _ => return None,
        },
        ("pastimes", _) if row["outcome"].as_str() == Some("completed") => {
            "leisure or creative activities completed"
        }
        ("agreement_expiry", "defaulted") => "unfulfilled agreements that expired",
        ("agreement_expiry", "withdrawn") => "unaccepted proposals that expired",
        ("personal_practices", "aid_proposed") => "individual aid proposals",
        ("personal_practices", "work_proposed") => "individual work and goods proposals",
        ("personal_practices", "aid_response" | "work_response") => {
            match (row["decision"].as_str()?, row["approved"].as_bool()) {
                ("adopted", Some(false)) => "practices adopted without private approval",
                ("adopted", Some(true)) => "practices adopted with private approval",
                ("declined", _) => "practices declined",
                _ => return None,
            }
        }
        ("personal_practices", "readiness_statement") => match row["belief"].as_str()? {
            "believes" => "readiness statements believed",
            "disbelieves" => "readiness statements doubted",
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
        .or_else(|| kind_value[category]["recipient"].as_u64())
        .or(person.filter(|id| Some(*id) != primary));
    Some(Receipt {
        label,
        actor: primary,
        other: secondary,
    })
}

pub(super) fn write_world(text: &mut String, chronicle: &Value) {
    if let Some(patterns) = chronicle["coordination_patterns"].as_array().filter(|rows| !rows.is_empty()) {
        text.push_str("## Recurring cooperation\n\n");
        for row in patterns {
            let crew_choices = row["crew_choices"].as_u64().unwrap_or(0);
            let delivery_ties = row["annual_delivery_ties"].as_u64().unwrap_or(0);
            let action = match (crew_choices > 0, delivery_ties > 0) {
                (true, true) => "received deliveries and was joined by helpers",
                (true, false) => "was joined by helpers",
                _ => "received deliveries",
            };
            write!(text, "- Person {} {action} in {} recorded years, spanning years {} through {}. Their peak was {} distinct collaborators in year {}.",
                row["person"], row["active_years"], row["first_year"], row["latest_year"], row["peak_collaborators"],
                row["peak_year"]).unwrap();
            if crew_choices > 0 {
                write!(text, " The archive recorded {crew_choices} accepted crew starts.").unwrap();
            }
            if delivery_ties > 0 {
                write!(text, " The archive recorded {delivery_ties} annual delivery relationships.").unwrap();
            }
            text.push('\n');
        }
        text.push('\n');
    }
    if let Some(rows) = chronicle["crew_turnover"].as_array().filter(|rows| !rows.is_empty()) {
        text.push_str("## Changes in who led crews\n\n");
        for row in rows {
            let workers = row["helpers"].as_array().into_iter().flatten()
                .filter_map(Value::as_u64).map(|id| id.to_string()).collect::<Vec<_>>();
            writeln!(text, "- People {} joined crews led most often by person {} in year {}, and by person {} in year {}.",
                workers.join(", "), row["previous_lead"], row["previous_year"], row["lead"], row["year"]).unwrap();
        }
        text.push('\n');
    }
    if let Some(traits) = chronicle["named_inherited_traits"].as_array().filter(|rows| !rows.is_empty()) {
        text.push_str("## Named inherited traits\n\n");
        for row in traits {
            let name = row["name"].as_str().unwrap_or("unnamed trait");
            let subject = row["species_name"].as_str().map_or_else(
                || format!("{name} in people"),
                |species| format!("{name} in {species}"),
            );
            write!(text, "- The mean allele value for {subject} was {} among {} living individuals in year {} and {} among {} in year {}. The latest sample with living individuals recorded {} with an allele pair different from the catalog default.",
                row["first"]["mean"], row["first"]["people"], row["first"]["year"], row["latest"]["mean"],
                row["latest"]["people"], row["latest"]["year"], row["latest"]["variants"]).unwrap();
            if let Some(year) = row["first_year_without_living_members"].as_u64() {
                write!(text, " The year-{year} checkpoint recorded no living members of this population.").unwrap();
            }
            text.push('\n');
        }
        text.push('\n');
    }
    if let Some(findings) = chronicle["recent_mechanical_findings"].as_array().filter(|rows| !rows.is_empty()) {
        text.push_str("## Recent mechanical findings\n\n");
        for row in findings {
            let evidence = &row["evidence"];
            let conditions = &evidence["conditions"];
            let (Some(moved), Some(stalled)) = (evidence["largest_load_moved"].as_u64(), evidence["smallest_load_stalled"].as_u64()) else { continue };
            let assessment = if moved < stalled { "The trials suggest a boundary between those loads." } else { "The two results overlap, so the comparison does not establish a boundary." };
            writeln!(text, "- In year {}, person {} compared trials on apparatus {} at output {} with input force {} and travel {}. The largest observed load that moved was {moved}. The smallest that stalled was {stalled}. {assessment}",
                row["year"], row["person"], conditions["apparatus"], conditions["output_part"],
                conditions["input_force"], conditions["input_distance"]).unwrap();
        }
        text.push('\n');
    }
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
    if let Some(rows) = chronicle["shared_crews"]
        .as_array()
        .filter(|rows| !rows.is_empty())
    {
        text.push_str("## Shared work\n\n");
        for row in rows.iter().take(5) {
            let helpers = row["helpers"].as_array().map_or(0, |rows| rows.len());
            let years = row["years"].as_array().map_or(0, Vec::len);
            writeln!(text, "- {helpers} people chose to help with person {}'s jobs in {years} recorded years. The first start was in year {} and the latest was in year {}. Their busiest year was {}, with {} starts from {} helpers.",
                row["lead"], row["first_year"], row["last_year"], row["peak"]["year"],
                row["peak"]["starts"], row["peak"]["helpers"]).unwrap();
        }
        text.push('\n');
    }
    if let Some(rows) = chronicle["recipe_transmission"]
        .as_array()
        .filter(|rows| !rows.is_empty())
    {
        text.push_str("## Recipe transmission\n\n");
        for row in rows.iter().take(5) {
            let name = row["name"]
                .as_str()
                .map_or_else(|| format!("recipe {}", row["recipe"]), str::to_owned);
            let teachers = row["teachers"].as_array().map_or(0, Vec::len);
            let learners = row["learners"].as_array().map_or(0, Vec::len);
            writeln!(text, "- {teachers} people taught {learners} learners {name}. The archive records those lessons from year {} through {}.", row["first_year"], row["last_year"]).unwrap();
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
    if let Some(episodes) = chronicle["recurring_supply"]
        .as_array()
        .filter(|rows| !rows.is_empty())
    {
        text.push_str("## Recurring sources of supply\n\n");
        for episode in episodes.iter().take(5) {
            let suppliers = episode["suppliers"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_u64)
                .map(|person| person.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            writeln!(text, "- Person {} received deliveries from people {suppliers} in successive years from {} through {}.", episode["recipient"], episode["first_year"], episode["latest_year"]).unwrap();
        }
        text.push('\n');
    }
    if let Some(entries) = chronicle["expedition_participation"]
        .as_array()
        .filter(|rows| !rows.is_empty())
    {
        text.push_str("## Expedition participation\n\n");
        for row in entries.iter().take(5) {
            let participants = row["participants"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_u64)
                .map(|person| person.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            writeln!(text, "- People {participants} joined expeditions on person {}'s invitation from year {} through {}. The archive records {} replies to join and {} replies to join under pressure.", row["inviter"], row["first_year"], row["latest_year"], row["joined"], row["joined_under_pressure"]).unwrap();
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
    fn written_history_distinguishes_adoption_adaptation_and_hearing() {
        let mut history = Chronicle::default();
        history.observe(
            &json!({
                "kind":"written_explanations", "at_millis":17,
                "events":[
                    {"speaker":1,"listener":2,"marks":[4],"adopted":{"sounds":[0],"marks":[4]}},
                    {"speaker":1,"listener":3,"marks":[4],"adopted":{"sounds":[0],"marks":[7]}},
                    {"speaker":1,"listener":4,"marks":[4],"heard":true}
                ]
            }),
            1,
            Some(3),
        );
        let summary = history.summary();
        let counts = &summary["counts"];
        assert_eq!(counts["written forms adopted after an explanation"], 1);
        assert_eq!(counts["written forms adapted after an explanation"], 1);
        assert_eq!(counts["written explanations without recorded adoption"], 1);
        let personal = history.selected();
        assert_eq!(personal.len(), 1);
        assert_eq!(personal[0]["actor"], 3);
        assert_eq!(personal[0]["other"], 1);
        assert_eq!(personal[0]["at_millis"], 17);
    }

    #[test]
    fn crew_counts_come_from_attributed_accepted_starts() {
        let mut history = Chronicle::default();
        let join = |person| json!({"kind":"crew_starts","at_millis":120,"events":[{"helper":person,"lead":1,"job":7,"recipe":1}]});
        history.observe(&join(2), 3, Some(2));
        history.observe(&join(3), 4, None);
        history.observe(&join(2), 4, None);
        let summary = history.summary();
        let crew = &summary["shared_crews"][0];
        assert_eq!(crew["helpers"], json!([2, 3]));
        assert_eq!(crew["starts"], 3);
        assert_eq!(crew["first_year"], 3);
        assert_eq!(crew["last_year"], 4);
        assert_eq!(crew["peak"], json!({"year":4,"starts":2,"helpers":2}));
        assert_eq!(
            crew["years"],
            json!([
                {"year":3,"starts":1,"helpers":[2]},
                {"year":4,"starts":2,"helpers":[2,3]}
            ]),
        );
        assert_eq!(history.selected()[0]["other"], 1);
    }

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

    #[test]
    fn reports_successive_years_of_deliveries_from_multiple_people_and_typed_supply_events() {
        let mut history = Chronicle::default();
        for (year, first, second) in [(0, 0, 0), (1, 1, 1), (2, 2, 2), (3, 3, 3), (4, 4, 3)] {
            history.snapshot(
                &json!({"agreements":{"contracts":[
                    {"id":1,"debtor":5,"creditor":11,"delivered":first},
                    {"id":2,"debtor":8,"creditor":11,"delivered":second},
                    {"id":3,"debtor":3,"creditor":7,"delivered":0}
                ]}}),
                year,
                year == 0,
            );
        }
        history.observe(
            &json!({"kind":"expeditions","events":[{
                "kind":{"supplies_acquired":{"good":1,"quantity":2,"from":{"person":1}}},
                "at_millis":48,"person":5,"itinerary":20,"region":1
            }]}),
            4,
            Some(5),
        );
        assert_eq!(
            history.summary()["recurring_supply"],
            json!([
                {"recipient":11,"suppliers":[5,8],"first_year":1,"latest_year":3}
            ])
        );
        assert_eq!(
            history.summary()["counts"]["expedition supplies acquired"],
            1
        );
        assert_eq!(history.selected()[0]["at_millis"], 48);
    }

    #[test]
    fn summarizes_actual_learning_and_does_not_attribute_passive_ripening_as_work() {
        let mut history = Chronicle::default();
        history.snapshot(
            &json!({"core":{"physical":{"recipes":[{"id":4,"name":"salt-roots"}]}}}),
            0,
            true,
        );
        for (year, pupil) in [(2, 9), (3, 9), (5, 10)] {
            history.observe(&json!({"kind":"recipe_learning","events":[{"recipe":4,"teacher":1,"student":pupil}]}), year, Some(9));
        }
        history.observe(&json!({"kind":"maturation","at_millis":500,"events":[{"outcome":"matured","place":{"kind":"person","id":9}}]}), 5, Some(9));
        let summary = history.summary();
        assert_eq!(
            summary["recipe_transmission"][0],
            json!({"recipe":4,"name":"salt-roots","first_year":2,"last_year":5,"teachers":[1],"learners":[9,10]})
        );
        assert_eq!(summary["counts"]["prepared batches matured"], 1);
        assert_eq!(summary["people"].as_array().unwrap().len(), 1);
        assert_eq!(history.selected().last().unwrap()["at_millis"], 500);
    }

    #[test]
    fn reports_participation_separately_from_refusal_and_pressure() {
        let mut history = Chronicle::default();
        let invitation = |recipient, response| json!({"person":5,"kind":{"invitation":{"recipient":recipient,"response":response}}});
        history.observe(&json!({"kind":"expeditions","events":[invitation(6, "declined"), invitation(3, "joined")]}), 2, None);
        history.observe(
            &json!({"kind":"expeditions","events":[invitation(8, "joined_under_pressure")]}),
            3,
            None,
        );
        let summary = history.summary();
        assert_eq!(summary["counts"]["expedition invitations declined"], 1);
        assert_eq!(
            summary["expedition_participation"],
            json!([{
                "inviter":5,"participants":[3,8],"joined":1,"joined_under_pressure":1,
                "first_year":2,"latest_year":3
            }])
        );
    }
}
