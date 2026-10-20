use super::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Exclusive,
    Companion,
    CoParent,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bonds {
    pub partners: Table<PersonId, Table<PersonId, Kind>>,
    pub caregivers: Table<PersonId, Table<PersonId, ()>>,
}
impl Resource for Bonds {
    const KEY: &'static str = "family-life.bonds";
    fn validate(&self) -> Result<()> {
        require(
            self.partners.iter().all(|(p, row)| {
                row.iter()
                    .all(|(q, k)| p != q && self.partners.get(q).and_then(|r| r.get(p)) == Some(k))
            }),
            "asymmetric bonds",
        )
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Bond {
    pub person: PersonId,
    pub other: PersonId,
    pub kind: Kind,
}
impl Request for Bond {
    const KEY: &'static str = "family-life.bond";
    type Response = bool;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Separate {
    pub person: PersonId,
    pub other: PersonId,
}
impl Request for Separate {
    const KEY: &'static str = "family-life.separate";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct ShareCare {
    pub caregiver: PersonId,
    pub child: PersonId,
    pub accept: bool,
}
impl Request for ShareCare {
    const KEY: &'static str = "family-life.share-care";
    type Response = bool;
}
pub(super) fn value(tx: &Transaction<'_>, p: PersonId, concept: &str) -> i64 {
    tx.get::<game_relationships::Social>()
        .ok()
        .and_then(|s| s.values.get(&p)?.get(concept))
        .copied()
        .map_or(0, i64::from)
}
fn attraction(tx: &Transaction<'_>, p: PersonId, q: PersonId, kind: Kind) -> i64 {
    let r = tx
        .get::<game_relationships::Social>()
        .ok()
        .and_then(|s| s.relationships.get(&p)?.get(&q))
        .cloned()
        .unwrap_or_default();
    i64::from(r.affection * 2 + r.trust - r.fear * 3 - r.grievance * 3)
        + match kind {
            Kind::Exclusive => value(tx, p, "exclusivity"),
            Kind::CoParent => value(tx, p, "kinship"),
            Kind::Companion => value(tx, p, "companionship"),
        }
}
fn assent(tx: &Transaction<'_>, p: PersonId, q: PersonId, kind: Kind) -> bool {
    let bodies = tx.get::<game_physiology::Bodies>().unwrap();
    let pop = tx.get::<game_lifecycle::Population>().unwrap();
    let adult = bodies
        .people
        .get(&p)
        .is_some_and(|b| b.alive && tx.now().0.saturating_sub(b.born.0) >= pop.adult_age.0);
    let existing = tx.get::<Bonds>().unwrap().partners.get(&p);
    let exclusive = existing.is_some_and(|r| {
        r.iter()
            .any(|(other, k)| *other != q && (*k == Kind::Exclusive || kind == Kind::Exclusive))
    });
    let related = pop
        .families
        .get(&p)
        .is_some_and(|f| [f.mother, f.father].contains(&q))
        || pop
            .families
            .get(&q)
            .is_some_and(|f| [f.mother, f.father].contains(&p));
    adult && !exclusive && !related && attraction(tx, p, q, kind) > 40
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.resource::<Bonds>()?;
    b.request::<Bond>(|tx, r| {
        require(r.person != r.other, "self bond")?;
        require(
            game_transport::area(tx.get()?, r.person)? == game_transport::area(tx.get()?, r.other)?,
            "bond needs contact",
        )?;
        if !assent(tx, r.person, r.other, r.kind) || !assent(tx, r.other, r.person, r.kind) {
            return Ok(false);
        }
        if r.kind == Kind::Exclusive
            && !tx.call(game_households::Form {
                first: r.person,
                second: r.other,
            })?
        {
            return Ok(false);
        }
        for (p, q) in [(r.person, r.other), (r.other, r.person)] {
            tx.edit::<Bonds>()?
                .partners
                .entry(p)
                .or_default()
                .insert(q, r.kind);
        }
        Ok(true)
    })?;
    b.request::<Separate>(|tx, r| {
        for (p, q) in [(r.person, r.other), (r.other, r.person)] {
            if let Some(row) = tx.edit::<Bonds>()?.partners.get_mut(&p) {
                row.remove(&q);
            }
        }
        tx.call(game_households::Separate {
            person: r.person,
            other: r.other,
        })
    })?;
    b.request::<ShareCare>(|tx, r| {
        if r.accept {
            game_physiology::capable(tx, r.caregiver)?;
            require(
                r.caregiver != r.child
                    && game_transport::area(tx.get()?, r.caregiver)?
                        == game_transport::area(tx.get()?, r.child)?,
                "care needs contact",
            )?;
            if game_vitality::can_converse(tx, r.child)
                && attraction(tx, r.child, r.caregiver, Kind::Companion) < 0
            {
                return Ok(false);
            }
            tx.edit::<Bonds>()?
                .caregivers
                .entry(r.child)
                .or_default()
                .insert(r.caregiver, ());
        } else if let Some(row) = tx.edit::<Bonds>()?.caregivers.get_mut(&r.child) {
            row.remove(&r.caregiver);
        }
        Ok(true)
    })?;
    b.subscribe::<game_households::Formed>("family-life.record", |tx, e| {
        for (p, q) in [(e.first, e.second), (e.second, e.first)] {
            tx.edit::<Bonds>()?
                .partners
                .entry(p)
                .or_default()
                .insert(q, Kind::Exclusive);
        }
        Ok(())
    })?;
    game_choice::provider(b, "family-life.choices", |tx, e| {
        if !game_choice::collecting(tx, e.person) || !tx.get::<Households>()?.enabled {
            return Ok(());
        }
        let p = e.person;
        if let Some(carrier) = game_transport::carrier(tx, p)
            && game_vitality::can_walk(tx, p)
        {
            let regard = tx.get::<game_relationships::Social>()?.regard(p, carrier);
            let desire = value(tx, p, "independence")
                + game_vitality::disposition(tx, p, "risk-tolerance") / 20
                - regard;
            game_choice::offer(
                tx,
                p,
                "family:dismount",
                game_choice::motive(0, 0, desire, 5, 0),
                game_transport::Dismount { person: p },
            )?;
        }
        let existing: Vec<_> = tx
            .get::<Bonds>()?
            .partners
            .get(&p)
            .into_iter()
            .flat_map(|r| r.keys())
            .copied()
            .collect();
        for other in existing {
            let score = -attraction(tx, p, other, Kind::Companion);
            game_choice::offer(
                tx,
                p,
                format!("family:separate:{}", other.0),
                game_choice::motive(0, 0, score, 5, 0),
                Separate { person: p, other },
            )?;
        }
        if tx.get::<Households>()?.dependents.contains_key(&p) && game_vitality::can_walk(tx, p) {
            let desire = value(tx, p, "independence")
                + game_vitality::disposition(tx, p, "risk-tolerance") / 20
                - super::dependence(tx, p)
                - tx.get::<Households>()?
                    .dependents
                    .get(&p)
                    .map_or(0, |parent| {
                        tx.get::<game_relationships::Social>()
                            .map_or(0, |s| s.regard(p, *parent))
                    })
                - 50;
            game_choice::offer(
                tx,
                p,
                "family:independence",
                game_choice::motive(0, 0, desire, 1, 0),
                game_households::Independent { person: p },
            )?;
        }
        let Ok(area) = game_transport::area(tx.get()?, p) else {
            return Ok(());
        };
        let peers: Vec<_> = tx
            .get::<game_social_life::SocialLife>()?
            .residents
            .get(&area)
            .into_iter()
            .flat_map(|r| r.keys())
            .copied()
            .filter(|q| *q != p)
            .take(16)
            .collect();
        for other in peers {
            if tx
                .get::<Bonds>()?
                .partners
                .get(&p)
                .is_some_and(|r| r.contains_key(&other))
            {
                continue;
            }
            for (name, kind) in [
                ("exclusive", Kind::Exclusive),
                ("companion", Kind::Companion),
                ("coparent", Kind::CoParent),
            ] {
                if !assent(tx, p, other, kind) {
                    continue;
                }
                let score = attraction(tx, p, other, kind) / 2;
                game_choice::offer(
                    tx,
                    p,
                    format!("family:{name}:{}", other.0),
                    game_choice::motive(0, 0, score, 10, 0),
                    game_choice::Undertake {
                        person: p,
                        kind: "courtship".into(),
                        duration: HOUR,
                        request: Pending::immediate(Bond {
                            person: p,
                            other,
                            kind,
                        }),
                    },
                )?;
            }
            if tx.get::<Households>()?.dependents.contains_key(&other)
                && !tx
                    .get::<Bonds>()?
                    .caregivers
                    .get(&other)
                    .is_some_and(|r| r.contains_key(&p))
            {
                let desire = game_vitality::disposition(tx, p, "altruism")
                    * game_vitality::disposition(tx, p, "empathy")
                    / 10000
                    + attraction(tx, p, other, Kind::Companion) / 4
                    - 40;
                game_choice::offer(
                    tx,
                    p,
                    format!("family:share-care:{}", other.0),
                    game_choice::motive(0, 0, desire, 5, 0),
                    ShareCare {
                        caregiver: p,
                        child: other,
                        accept: true,
                    },
                )?;
            }
        }
        Ok(())
    })
}
