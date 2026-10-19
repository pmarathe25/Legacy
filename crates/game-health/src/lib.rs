//! Treatment requires a capable caregiver, local patient, supplies, and time.
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreatmentRules {
    pub supply_object: GoodId,
    pub duration: SimDuration,
    pub supply: u64,
    pub healing: u64,
}
impl Default for TreatmentRules {
    fn default() -> Self {
        Self {
            supply_object: game_cargo::MEDICINE,
            duration: HOUR,
            supply: 1,
            healing: 10,
        }
    }
}
impl Resource for TreatmentRules {
    const KEY: &'static str = "health.rules";
    fn validate(&self) -> Result<()> {
        require(self.duration.0 > 0 && self.supply > 0, "invalid treatment")
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Treat {
    pub caregiver: PersonId,
    pub patient: PersonId,
    pub supplies: InventoryId,
}
impl Request for Treat {
    const KEY: &'static str = "health.treat";
    type Response = SimTime;
}
#[derive(Clone, Serialize, Deserialize)]
struct Finish {
    caregiver: PersonId,
    patient: PersonId,
}
impl Request for Finish {
    const KEY: &'static str = "health.finish";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Cancel {
    job: ScheduleId,
}
impl Request for Cancel {
    const KEY: &'static str = "health.cancel";
    type Response = ();
}
#[derive(Serialize, Deserialize)]
pub struct Treated {
    pub caregiver: PersonId,
    pub patient: PersonId,
    pub successful: bool,
}
impl Event for Treated {
    const KEY: &'static str = "health.treated";
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Spec {
    supply_object: String,
    duration: SimDuration,
    supply: u64,
    healing: u64,
}
pub fn load(
    store: &mut game_content::Store,
    key: &str,
    objects: &game_objects::Objects,
) -> Result<TreatmentRules> {
    let doc: game_content::Document<Spec> = store.load(&format!("health/{key}.toml"))?;
    let r = doc.data;
    let rules = TreatmentRules {
        supply_object: objects.resolve(&r.supply_object)?,
        duration: r.duration,
        supply: r.supply,
        healing: r.healing,
    };
    rules.validate()?;
    Ok(rules)
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Configure(pub TreatmentRules);
impl Request for Configure {
    const KEY: &'static str = "health.configure";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct ForceTreat(pub Treat);
impl Request for ForceTreat {
    const KEY: &'static str = "health.force-treat";
    type Response = SimTime;
}
#[derive(Clone, Serialize, Deserialize)]
struct Begin {
    treatment: Treat,
    forced: bool,
}
impl Request for Begin {
    const KEY: &'static str = "health.begin";
    type Response = SimTime;
}
fn accepts(tx: &Transaction<'_>, r: &Treat) -> bool {
    if r.caregiver == r.patient {
        return true;
    }
    let bodies = tx.get::<game_physiology::Bodies>().unwrap();
    let Some(b) = bodies.people.get(&r.patient) else {
        return false;
    };
    let need = b.maximum(&bodies.rules).saturating_sub(b.health).min(100) as i64;
    let regard = tx
        .get::<game_relationships::Social>()
        .ok()
        .and_then(|s| s.relationships.get(&r.patient)?.get(&r.caregiver))
        .map_or(0, |r| {
            i64::from(r.trust + r.affection / 2 - r.grievance - r.fear)
        });
    let interruption = if tx.get::<Actions>().unwrap().0.contains_key(&r.patient) {
        20
    } else {
        0
    };
    need * 3 + regard
        > interruption + game_vitality::disposition(tx, r.patient, "risk-tolerance") / 50
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("health", 1)?;
    b.resource::<TreatmentRules>()?;
    b.request::<Configure>(|tx, r| {
        r.0.validate()?;
        *tx.edit::<TreatmentRules>()? = r.0;
        Ok(())
    })?;
    b.request::<Treat>(|tx, r| {
        tx.call(Begin {
            treatment: r,
            forced: false,
        })
    })?;
    b.request::<ForceTreat>(|tx, r| {
        tx.call(Begin {
            treatment: r.0,
            forced: true,
        })
    })?;
    b.request::<Begin>(|tx, request| {
        let r = request.treatment;
        if request.forced {
            require(
                tx.registered::<game_physiology::Restrained>()
                    && tx.call(game_physiology::Restrained {
                        actor: r.caregiver,
                        patient: r.patient,
                    })?,
                "forced treatment needs physical restraint",
            )?;
        } else {
            require(accepts(tx, &r), "patient declines treatment")?;
        }
        game_physiology::capable(tx, r.caregiver)?;
        require(
            game_vitality::can_manipulate(tx, r.caregiver),
            "treatment exceeds motor capacity",
        )?;
        require(
            tx.get::<game_physiology::Bodies>()?
                .people
                .get(&r.patient)
                .is_some_and(|p| p.alive),
            "patient is dead or unknown",
        )?;
        let area = game_transport::area(tx.get()?, r.caregiver)?;
        require(
            game_transport::area(tx.get()?, r.patient)? == area
                && game_cargo::locality(tx, r.supplies)? == area,
            "care is not local",
        )?;
        tx.call(game_property::Use {
            actor: r.caregiver,
            inventory: r.supplies,
            purpose: "treatment".into(),
        })?;
        let rules = tx.get::<TreatmentRules>()?.clone();
        tx.call(game_cargo::Consume {
            inventory: r.supplies,
            good: rules.supply_object,
            quantity: rules.supply,
        })?;
        let until = tx.now().after(rules.duration)?;
        let job = tx.schedule(
            until,
            10,
            Finish {
                caregiver: r.caregiver,
                patient: r.patient,
            },
        )?;
        tx.start_action(r.caregiver, "treat", Some(until), Cancel { job })?;
        Ok(until)
    })?;
    b.request::<Finish>(|tx, r| {
        let local = game_transport::area(tx.get()?, r.caregiver)
            .ok()
            .zip(game_transport::area(tx.get()?, r.patient).ok())
            .is_some_and(|(a, b)| a == b);
        let successful = local && tx.get::<game_physiology::Bodies>()?.people[&r.patient].alive;
        if successful {
            let healing = tx.get::<TreatmentRules>()?.healing;
            tx.call(game_physiology::Heal {
                person: r.patient,
                amount: healing,
            })?;
        }
        tx.finish_action(r.caregiver, "treat")?;
        tx.emit(
            Treated {
                caregiver: r.caregiver,
                patient: r.patient,
                successful,
            },
            vec![r.caregiver, r.patient],
        )
    })?;
    b.request::<Cancel>(|tx, r| {
        tx.unschedule(r.job);
        Ok(())
    })
}
