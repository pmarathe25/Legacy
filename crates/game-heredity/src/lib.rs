//! Genes express characteristics. Decisions consume traits, never genomes.
use game_core::*;
use game_sim::*;
use game_storage::{Records, Table};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Heredity {
    pub loci: Records<PersonId, BTreeMap<String, game_genetics::Alleles>>,
    pub generation: Table<PersonId, u32>,
    pub maximum: u32,
    pub living: u64,
}
impl Resource for Heredity {
    const KEY: &'static str = "heredity.loci";
    fn validate(&self) -> Result<()> {
        require(
            self.loci.keys().all(|p| self.generation.contains_key(p)),
            "missing generation",
        )?;
        require(
            self.maximum == self.generation.values().copied().max().unwrap_or(0),
            "invalid ancestry depth",
        )
    }
    fn validate_changes(&self, old: &Self) -> Result<()> {
        let changed = self
            .generation
            .changed(&old.generation)
            .map(|(_, g)| *g)
            .max()
            .unwrap_or(0);
        require(
            self.maximum == old.maximum.max(changed),
            "invalid ancestry update",
        )?;
        require(
            self.loci
                .changed(&old.loci)
                .all(|(p, _)| self.generation.contains_key(p)),
            "missing generation",
        )
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Founder {
    pub person: PersonId,
}
impl Request for Founder {
    const KEY: &'static str = "heredity.founder";
    type Response = ();
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("heredity", 2)?;
    b.resource::<Heredity>()?;
    b.request::<Founder>(|tx, r| {
        require(
            !tx.get::<Heredity>()?.loci.contains_key(&r.person),
            "already genotyped",
        )?;
        let traits = tx.get::<game_traits::Traits>()?;
        let loci = traits
            .definitions
            .iter()
            .map(|(key, t)| {
                let v = traits
                    .people
                    .get(&r.person)
                    .and_then(|p| p.get(key))
                    .copied()
                    .unwrap_or(t.default);
                let n = ((i64::from(v) - i64::from(t.minimum)) * 65535
                    / (i64::from(t.maximum) - i64::from(t.minimum))) as u16;
                let stream = key
                    .bytes()
                    .fold(0u64, |n, c| n.wrapping_mul(31).wrapping_add(u64::from(c)));
                (
                    key.clone(),
                    game_genetics::founder(
                        random(tx.seed(), stream, 901),
                        r.person,
                        n,
                        t.founder_spread,
                    ),
                )
            })
            .collect();
        let mortal = tx
            .get::<game_physiology::Bodies>()?
            .people
            .get(&r.person)
            .is_some_and(|p| p.alive && p.kind == game_physiology::Kind::Mortal);
        let s = tx.edit::<Heredity>()?;
        s.loci.insert(r.person, loci);
        s.generation.insert(r.person, 0);
        s.living += u64::from(mortal);
        tx.call(game_physiology::Express { person: r.person })?;
        Ok(())
    })?;
    b.subscribe::<game_lifecycle::Born>("a.heredity.expression", |tx, e| {
        let family = &tx.get::<game_lifecycle::Population>()?.families[&e.person];
        let (mother, father) = (family.mother, family.father);
        let state = tx.get::<Heredity>()?;
        let (Some(m), Some(f)) = (state.loci.get(&mother), state.loci.get(&father)) else {
            return Ok(());
        };
        let generation = state.generation[&mother]
            .min(state.generation[&father])
            .saturating_add(1);
        let mut profile = BTreeMap::new();
        let mut loci = BTreeMap::new();
        for (key, t) in &tx.get::<game_traits::Traits>()?.definitions {
            let default = game_genetics::Alleles([32767, 32767]);
            let alleles = game_genetics::inherit_bounded(
                random(
                    tx.seed(),
                    key.bytes()
                        .fold(0u64, |n, c| n.wrapping_mul(31).wrapping_add(u64::from(c))),
                    900,
                ),
                e.person,
                *m.get(key).unwrap_or(&default),
                *f.get(key).unwrap_or(&default),
                500,
                u16::MAX,
            );
            let expression = game_genetics::expression(alleles);
            let value = i64::from(t.minimum)
                + i64::from(expression) * (i64::from(t.maximum) - i64::from(t.minimum)) / 65535;
            profile.insert(key.clone(), value as i32);
            loci.insert(key.clone(), alleles);
        }
        tx.call(game_traits::Set {
            person: e.person,
            profile,
        })?;
        let state = tx.edit::<Heredity>()?;
        state.loci.insert(e.person, loci);
        state.generation.insert(e.person, generation);
        state.maximum = state.maximum.max(generation);
        state.living += 1;
        tx.call(game_physiology::Express { person: e.person })?;
        Ok(())
    })?;
    b.subscribe::<game_physiology::Died>("heredity.cohort", |tx, e| {
        if tx.get::<Heredity>()?.loci.contains_key(&e.person)
            && tx.get::<game_physiology::Bodies>()?.people[&e.person].kind
                == game_physiology::Kind::Mortal
        {
            tx.edit::<Heredity>()?.living = tx
                .get::<Heredity>()?
                .living
                .checked_sub(1)
                .ok_or(Error::Overflow)?;
        }
        Ok(())
    })?;
    b.validate_save(|h| {
        let traits = h.get::<game_traits::Traits>()?;
        let s = h.get::<Heredity>()?;
        let living = s
            .loci
            .keys()
            .filter(|p| {
                h.get::<game_physiology::Bodies>().is_ok_and(|b| {
                    b.people
                        .get(p)
                        .is_some_and(|p| p.alive && p.kind == game_physiology::Kind::Mortal)
                })
            })
            .count() as u64;
        require(s.living == living, "invalid living cohort count")?;
        for (p, loci) in &h.get::<Heredity>()?.loci {
            require(
                h.get::<game_physiology::Bodies>()?.people.contains_key(p),
                "genome without person",
            )?;
            require(
                loci.keys().all(|k| traits.definitions.contains_key(k)),
                "unknown genetic trait",
            )?;
        }
        Ok(())
    });
    Ok(())
}
