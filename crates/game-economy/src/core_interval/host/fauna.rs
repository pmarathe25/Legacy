//! Animal lifecycle and grazing against the resources left after gathering.
use super::*;

pub(super) enum CoreFaunaDraft {
    Quiet(game_fauna::QuietDraft),
    Complete(game_fauna::Fauna),
    Encounters(game_fauna::Encounters),
}

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct CoreFaunaTransitions {
    pub at_millis: u128,
    pub births: Vec<u64>,
    pub deaths: Vec<game_fauna::AnimalDeath>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub migrations: Vec<game_fauna::dispersal::Migration>,
}

impl CoreHost {
    pub(super) fn next_fauna_lifecycle(&self) -> Option<u64> {
        self.fauna
            .as_ref()?
            .next_lifecycle_after(self.core.simulated_millis())
    }

    pub(super) fn stage_fauna(
        &self,
        gathering: &mut Option<crate::Gathering>,
        hunted: &[u64],
        feeding: &[game_fauna::Feeding],
        tick: u64,
    ) -> Result<Option<CoreFaunaDraft>, String> {
        let Some(owner) = &self.fauna else {
            return Ok(None);
        };
        if owner.lifecycle_period_millis.is_some() {
            if hunted.is_empty() && feeding.is_empty() && owner.recent_deaths.is_empty() {
                return Ok(None);
            }
            return owner
                .prepare_encounters(hunted, feeding)
                .map(|draft| Some(CoreFaunaDraft::Encounters(draft)))
                .map_err(str::to_owned);
        }
        let season = self.settlement_season(tick);
        let nodes = gathering.as_mut().map(|owner| owner.nodes.as_mut_slice());
        self.stage_fauna_lifecycle(owner, nodes, hunted, feeding, season, tick)
            .map(Some)
    }

    pub(super) fn stage_elapsed_fauna(
        &self,
        renewal: &mut Option<Vec<crate::ResourceNode>>,
        at_millis: u128,
    ) -> Result<(CoreFaunaDraft, CoreFaunaTransitions), String> {
        let owner = self.fauna.as_ref().expect("scheduled fauna");
        let ordinal = owner
            .last_tick
            .checked_add(1)
            .ok_or("fauna lifecycle overflow")?;
        let season = self.environment.as_ref().and_then(|environment| {
            environment.season_at(at_millis - 1, self.core.physical.tick())
        });
        let mut draft =
            self.stage_fauna_lifecycle(owner, renewal.as_deref_mut(), &[], &[], season, ordinal)?;
        let restrained = |animal| {
            self.animal_restraints
                .as_ref()
                .is_some_and(|state| state.held(animal))
        };
        let migrations = match &mut draft {
            CoreFaunaDraft::Quiet(quiet) => owner.disperse_quiet(quiet, self.core.seed, restrained),
            CoreFaunaDraft::Complete(next) => {
                next.disperse_living(owner.next_id, self.core.seed, restrained)
            }
            CoreFaunaDraft::Encounters(_) => unreachable!("ecological event without lifecycle"),
        };
        let (births, deaths) = match &draft {
            CoreFaunaDraft::Quiet(quiet) => {
                (quiet.births().collect(), quiet.death_receipts().to_vec())
            }
            CoreFaunaDraft::Complete(next) => (
                next.animals
                    .par_iter()
                    .filter(|animal| animal.id >= owner.next_id)
                    .map(|animal| animal.id)
                    .collect(),
                next.recent_deaths.clone(),
            ),
            CoreFaunaDraft::Encounters(_) => unreachable!("ecological event without lifecycle"),
        };
        Ok((
            draft,
            CoreFaunaTransitions {
                at_millis,
                births,
                deaths,
                migrations,
            },
        ))
    }

    fn stage_fauna_lifecycle(
        &self,
        owner: &game_fauna::Fauna,
        nodes: Option<&mut [crate::ResourceNode]>,
        hunted: &[u64],
        feeding: &[game_fauna::Feeding],
        season: Option<&game_environment::Season>,
        tick: u64,
    ) -> Result<CoreFaunaDraft, String> {
        let wild = self.gathering.as_ref().and_then(|owner| owner.wild_plants.as_ref());
        let patches: Vec<_> = nodes
            .as_deref()
            .unwrap_or_default()
            .par_iter()
            .filter(|node| wild.is_none_or(|owner| owner.stand(node.id).is_none()))
            .map(|node| game_fauna::Patch {
                id: node.id,
                region: node.region,
                good: node.good,
                available: node.available,
            })
            .collect();
        let exposure: Vec<_> = self
            .environment
            .as_ref()
            .map_or(&[][..], |owner| owner.regions.as_slice())
            .par_iter()
            .map(|hazard| game_fauna::Exposure {
                region: hazard.region,
                severity: season.map_or(hazard.damage, |season| season.exposure(hazard.damage)),
            })
            .collect();
        let quiet = owner
            .prepare_compact(&patches, feeding, &exposure, hunted, self.core.seed, tick)
            .map_err(str::to_owned)?;
        let (draft, consumed) = if let Some(quiet) = quiet {
            owner.validate_quiet(&quiet).map_err(str::to_owned)?;
            let consumed: Vec<_> = quiet.consumed().par_iter().copied().collect();
            (CoreFaunaDraft::Quiet(quiet), consumed)
        } else {
            let complete = owner
                .advance_with_hunts(&patches, feeding, &exposure, hunted, self.core.seed, tick)
                .map_err(str::to_owned)?;
            (CoreFaunaDraft::Complete(complete.fauna), complete.consumed)
        };
        if !consumed.is_empty() {
            let resources = nodes.ok_or("grazing requires resource owner")?;
            resources
                .par_iter_mut()
                .try_for_each(|node| -> Result<(), String> {
                    if let Ok(index) = consumed.binary_search_by_key(&node.id, |row| row.0) {
                        node.available = node
                            .available
                            .checked_sub(consumed[index].1)
                            .ok_or("grazing exceeds resource")?;
                    }
                    Ok(())
                })?;
        }
        Ok(draft)
    }

    pub(super) fn publish_fauna(&mut self, draft: Option<CoreFaunaDraft>) {
        match draft {
            Some(CoreFaunaDraft::Quiet(draft)) => self
                .fauna
                .as_mut()
                .expect("staged fauna")
                .publish_quiet_validated(draft),
            Some(CoreFaunaDraft::Complete(fauna)) => self.fauna = Some(fauna),
            Some(CoreFaunaDraft::Encounters(draft)) => self
                .fauna
                .as_mut()
                .expect("staged fauna")
                .publish_encounters(draft),
            None => {}
        }
    }

    pub(super) fn publish_elapsed_fauna(
        &mut self,
        draft: Option<CoreFaunaDraft>,
        transitions: Option<&CoreFaunaTransitions>,
    ) {
        if transitions.is_some()
            && let Some(products) = self.animal_products.as_mut().filter(|owner| owner.elapsed_actions)
        {
            products.renew_at_lifecycle(self.fauna.as_ref().expect("elapsed livestock"));
        }
        self.publish_fauna(draft);
        if transitions.is_some_and(|events| !events.deaths.is_empty())
            && let Some(restraints) = &mut self.animal_restraints
        {
            restraints.reconcile(
                &self.core.people,
                self.fauna.as_ref().expect("elapsed fauna"),
                &|_| true,
                self.core.physical.tick(),
            );
        }
    }
}
