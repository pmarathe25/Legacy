use super::*;
#[derive(Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct Ledger {
    pub(crate) sequence: u64,
    #[serde(default)]
    pub(crate) plants: crate::plants::Plants,
    #[serde(default)]
    pub(crate) founding_sequence: u64,
    pub(crate) epoch: u64,
    #[serde(default)]
    pub(crate) notable: bool,
    #[serde(default)]
    pub(crate) journal_year: u64,
    #[serde(default)]
    pub(crate) seen: BTreeSet<String>,
    #[serde(default)]
    pub(crate) seasonal: BTreeSet<String>,
    #[serde(default)]
    pub(crate) feeding: BTreeMap<PersonId, Feeding>,
    pub(crate) counts: BTreeMap<String, u64>,
    pub(crate) crafted: BTreeMap<String, u64>,
    pub(crate) harvested: BTreeMap<String, u64>,
    pub(crate) crop_failures: u64,
    pub(crate) novel: BTreeSet<u64>,
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct Feeding {
    at: SimTime,
    caregiver: PersonId,
    good: GoodId,
    quantity: u64,
    count: u64,
}
impl Ledger {
    pub(crate) fn collect(&mut self, h: &game_sim::Host, out: &mut impl Write) -> Result<()> {
        let objects = h.get::<game_objects::Objects>()?;
        let after = self.sequence;
        // Ingestion can publish death before its Fed notice, within one commit.
        let mut committed_feeds: BTreeMap<PersonId, (u64, Feeding)> = BTreeMap::new();
        if h.notices_since(after).any(|e| e.kind == "physiology.died") {
            for event in h
                .notices_since(after)
                .filter(|e| e.kind == "physiology.fed")
            {
                let e: game_physiology::Fed = serde_json::from_value(event.data.clone())?;
                let count = committed_feeds
                    .get(&e.patient)
                    .map(|(_, f)| f)
                    .or_else(|| self.feeding.get(&e.patient))
                    .map_or(1, |f| f.count + 1);
                committed_feeds.insert(
                    e.patient,
                    (
                        event.sequence,
                        Feeding {
                            at: event.at,
                            caregiver: e.caregiver,
                            good: e.good,
                            quantity: e.quantity,
                            count,
                        },
                    ),
                );
            }
        }
        for event in h.notices_since(after) {
            if event.sequence != self.sequence + 1 {
                return Err("chronicle lost events; reduce advance budget".into());
            }
            self.sequence = event.sequence;
            *self.counts.entry(event.kind.clone()).or_default() += 1;
            let mut significant = matches!(
                event.kind.as_str(),
                "physiology.died"
                    | "names.changed"
                    | "names.called"
                    | "communities.founded"
                    | "pressure.submitted"
                    | "pressure.backed-down"
                    | "pressure.fled"
                    | "lifecycle.conceived"
                    | "lifecycle.born"
                    | "health.treated"
                    | "agriculture.harvested"
                    | "wayfaring.visited"
                    | "property.interfered"
                    | "grievances.response"
                    | "agriculture.encroached"
                    | "institutions.rule-adopted"
                    | "physiology.fed"
                    | "physiology.damaged"
                    | "conflict.struck"
                    | "households.formed"
                    | "households.fostered"
                    | "exchange.traded"
                    | "discourse.spoken"
                    | "communication.learned"
                    | "syntax.learned"
                    | "mechanics.built"
                    | "mechanics.tested"
                    | "literacy.developed"
                    | "households.care-changed"
                    | "care.feeding-failed"
                    | "affiliation.arrangement-discussed"
                    | "works.created"
                    | "works.shared"
                    | "institutions.membership-changed"
                    | "institutions.procedure-changed"
                    | "equipment.exposed"
                    | "exposure.changed"
                    | "shelter.built"
                    | "shelter.fire-lit"
                    | "food-storage.built"
                    | "maturation.matured"
                    | "botany.changed"
                    | "cultivars.created"
                    | "nutrition.withdrawal"
                    | "items.marked"
                    | "sentiment.changed"
                    | "items.moved"
                    | "sentiment.lost"
            );
            if event.kind == "workshop.produced" || event.kind == "agriculture.harvested" {
                let good = event.data["good"].as_u64().ok_or("event lacks good")?;
                let name = objects.by_id(GoodId(good))?.name.clone();
                let amount = event.data["quantity"]
                    .as_u64()
                    .ok_or("event lacks quantity")?;
                if event.kind == "workshop.produced" {
                    *self.crafted.entry(name).or_default() += amount;
                    significant |= self.novel.insert(good);
                } else {
                    *self.harvested.entry(name).or_default() += amount;
                    self.crop_failures += u64::from(amount == 0);
                }
            }
            if self.notable {
                let year = event.at.0.saturating_sub(self.epoch) / (365 * DAY.0);
                if year != self.journal_year {
                    self.seasonal.clear();
                    self.journal_year = year;
                }
                match event.kind.as_str() {
                    "care.feeding-failed" => {
                        significant = self.seasonal.insert(format!(
                            "care-failed:{}:{}:{}:{}",
                            event.data["caregiver"],
                            event.data["patient"],
                            event.data["good"],
                            event.data["reason"],
                        ));
                    }
                    "affiliation.arrangement-discussed" => {
                        significant = self.seasonal.insert(format!(
                            "arrangement:{}:{}",
                            event.data["proposer"], event.data["institution"],
                        ));
                    }
                    "cultivars.created" => significant = false,
                    "households.care-changed" => {
                        significant = self.seen.insert(format!(
                            "care:{}:{}:{}",
                            event.data["caregiver"],
                            event.data["beneficiary"],
                            event.data["accept"]
                        ));
                    }
                    "mechanics.built" => {
                        let id = event.data["device"].as_u64().ok_or("apparatus lacks id")?;
                        let device = &h.get::<game_mechanics::Machines>()?.devices[&id];
                        significant = self.seen.insert(format!(
                            "design:{}:{}",
                            device.maker.0,
                            game_physics::design_key(&device.parts)
                        ));
                    }
                    "mechanics.tested" => {
                        let load = event.data["lifted"].as_u64().unwrap_or(0);
                        let met = event.data["task"].get(1).map(|task| {
                            load >= task["force"].as_u64().unwrap_or(u64::MAX)
                                && event.data["travel"].as_u64().unwrap_or(0)
                                    >= task["travel"].as_u64().unwrap_or(u64::MAX)
                        });
                        significant = !event.data["device"].is_null()
                            && self.seen.insert(format!(
                                "trial:{}:{}:{}:{}:{:?}",
                                event.data["device"],
                                event.data["outlet"],
                                event.data["task"].get(0).unwrap_or(&Value::Null),
                                load.checked_ilog2().unwrap_or(0),
                                met
                            ));
                    }

                    "discourse.spoken" => {
                        significant = self.seen.insert(format!(
                            "opinion:{}:{}:{}:{}",
                            event.data["speaker"],
                            event.data["concept"],
                            event.data["stance"].as_i64().unwrap_or(0).div_euclid(25),
                            event.data["coerced"]
                        ));
                    }
                    "botany.changed" => {
                        let patch = &event.data["patch"];
                        let empty = event.data["living"] == 0;
                        significant = self.seasonal.insert(format!("plants:{patch}:{empty}"));
                    }
                    "nutrition.withdrawal" => {
                        significant = self
                            .seen
                            .insert(format!("withdrawal:{}", event.data["person"]))
                    }
                    "exposure.changed" => {
                        significant = self.seen.insert(format!(
                            "thermal:{}:{}",
                            event.data["person"],
                            event.data["core"].as_i64().unwrap_or(37000) / 1000
                        ))
                    }
                    "physiology.damaged" => {
                        significant = self
                            .seasonal
                            .insert(format!("injured:{}", event.data["person"]))
                    }
                    "conflict.struck" => {
                        let fatal = event.data["target"].as_u64().is_some_and(|id| {
                            h.get::<game_physiology::Bodies>().is_ok_and(|b| {
                                b.people.get(&PersonId(id)).is_some_and(|body| !body.alive)
                            })
                        });
                        significant = fatal
                            || self.seen.insert(format!(
                                "struck:{}:{}",
                                event.data["actor"], event.data["target"]
                            ));
                    }
                    "pressure.submitted" | "pressure.backed-down" => {
                        significant = self.seen.insert(format!(
                            "{}:{}:{}",
                            event.kind, event.data["leader"], event.data["subject"]
                        ))
                    }
                    "pressure.fled" => {
                        significant = self.seen.insert(format!(
                            "fled:{}:{}:{}",
                            event.data["person"], event.data["other"], event.data["destination"]
                        ))
                    }
                    "health.treated" => {
                        significant = self.seen.insert(format!(
                            "treated:{}:{}:{}",
                            event.data["caregiver"],
                            event.data["patient"],
                            event.data["successful"]
                        ))
                    }
                    "shelter.fire-lit" => {
                        significant = self.seasonal.insert(format!(
                            "fire:{}:{}",
                            event.data["person"], event.data["room"]
                        ))
                    }
                    "maturation.matured" => {
                        significant = self.seen.insert(format!(
                            "matured:{}:{}",
                            event.data["output"],
                            event.data["lost"].as_u64().unwrap_or(0) > 0
                        ))
                    }
                    "physiology.fed" => {
                        let patient = PersonId(
                            event.data["patient"]
                                .as_u64()
                                .ok_or("feeding lacks patient")?,
                        );
                        let caregiver = PersonId(
                            event.data["caregiver"]
                                .as_u64()
                                .ok_or("feeding lacks caregiver")?,
                        );
                        let good = GoodId(event.data["good"].as_u64().ok_or("feeding lacks good")?);
                        let count = self.feeding.get(&patient).map_or(1, |f| f.count + 1);
                        self.feeding.insert(
                            patient,
                            Feeding {
                                at: event.at,
                                caregiver,
                                good,
                                quantity: event.data["quantity"].as_u64().unwrap_or(0),
                                count,
                            },
                        );
                        significant = self
                            .seen
                            .insert(format!("fed:{}:{}:{}", caregiver.0, patient.0, good.0));
                    }
                    "property.interfered" => {
                        significant = self.seen.insert(format!(
                            "use:{}:{}:{}",
                            event.data["actor"], event.data["inventory"], event.data["purpose"]
                        ))
                    }
                    "grievances.response" => {
                        significant = self.seen.insert(format!(
                            "response:{}:{}:{}:{}",
                            event.data["observer"],
                            event.data["actor"],
                            event.data["inventory"],
                            event.data["forgiven"]
                        ))
                    }
                    "exchange.traded" => {
                        significant = self.seen.insert(format!(
                            "trade:{}:{}:{}:{}",
                            event.data["first"],
                            event.data["second"],
                            event.data["offered"],
                            event.data["requested"]
                        ))
                    }
                    "preservation.spoiled" => {
                        significant = self.seen.insert(format!("spoiled:{}", event.data["good"]))
                    }
                    "agreements.defaulted" => significant = true,
                    _ => {}
                }
            }
            if significant {
                let mut data = event.data.clone();
                if event.kind == "mechanics.built" {
                    let id = data["device"].as_u64().ok_or("apparatus lacks id")?;
                    data["design"] =
                        serde_json::to_value(&h.get::<game_mechanics::Machines>()?.devices[&id])?;
                }

                if event.kind == "botany.changed" {
                    self.plants.enrich(h, &mut data)?;
                    if self.notable
                        && let Some(plants) = data["observed_plants"].as_array_mut()
                    {
                        let count = plants.len();
                        plants.truncate(4);
                        data["observed_plant_count"] = json!(count);
                    }
                }
                if self.notable
                    && let Some(witnesses) = data.get_mut("witnesses").and_then(Value::as_array_mut)
                {
                    witnesses.truncate(4);
                }
                let names: BTreeMap<_, _> = event
                    .witnesses
                    .iter()
                    .take(if self.notable { 4 } else { usize::MAX })
                    .filter_map(|p| {
                        h.get::<game_social_life::SocialLife>()
                            .ok()?
                            .people
                            .get(p)
                            .map(|n| (p.0, n.name.clone()))
                    })
                    .collect();
                let body = if event.kind == "physiology.died" {
                    event
                        .data
                        .get("body")
                        .filter(|v| !v.is_null())
                        .cloned()
                        .unwrap_or_else(|| {
                            serde_json::to_value(event.data["person"].as_u64().and_then(|p| {
                                h.get::<game_physiology::Bodies>().ok()?.body(&PersonId(p))
                            }))
                            .unwrap()
                        })
                } else {
                    Value::Null
                };
                let mut row = json!({"day":event.at.0.saturating_sub(self.epoch) as f64/DAY.0 as f64,"sequence":event.sequence,"kind":event.kind,"data":data,"witnesses":names,"witness_count":event.witnesses.len(),"body":body});
                if self.notable && event.kind == "physiology.died" {
                    let p = event.data["person"].as_u64().map(PersonId);
                    let committed = p.and_then(|p| committed_feeds.get(&p));
                    row["last_feed"] = serde_json::to_value(
                        committed
                            .map(|(_, f)| f)
                            .or_else(|| p.and_then(|p| self.feeding.get(&p))),
                    )?;
                    row["ingestion_reported_after_death_notice"] =
                        json!(committed.is_some_and(|(seq, _)| *seq > event.sequence));
                }
                serde_json::to_writer(&mut *out, &row)?;
                writeln!(out)?;
            }
        }
        Ok(())
    }
    pub(crate) fn value(&self) -> Value {
        json!({"compact_policy":2,"founding_sequence":self.founding_sequence,"events":self.counts,"crafted":self.crafted,"harvested":self.harvested,"crop_failures":self.crop_failures})
    }
}
