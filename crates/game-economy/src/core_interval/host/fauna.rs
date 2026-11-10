//! Animal lifecycle and grazing against the resources left after gathering.
use super::*;

pub(super) enum CoreFaunaDraft {
    Quiet(game_fauna::QuietDraft),
    Complete(game_fauna::Fauna),
}

impl CoreHost {
    pub(super) fn stage_fauna(
        &self,
        gathering: &mut Option<crate::Gathering>,
        hunted: &[u64],
        tick: u64,
    ) -> Result<Option<CoreFaunaDraft>, String> {
        let Some(owner) = &self.fauna else {
            return Ok(None);
        };
        let patches: Vec<_> = gathering
            .as_ref()
            .map_or(&[][..], |owner| owner.nodes.as_slice())
            .par_iter()
            .map(|node| game_fauna::Patch {
                id: node.id,
                region: node.region,
                good: node.good,
                available: node.available,
            })
            .collect();
        let season = self
            .environment
            .as_ref()
            .and_then(|owner| owner.season(tick));
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
            .prepare_compact(&patches, &[], &exposure, hunted, self.core.seed, tick)
            .map_err(str::to_owned)?;
        let (draft, consumed) = if let Some(quiet) = quiet {
            owner.validate_quiet(&quiet).map_err(str::to_owned)?;
            let consumed: Vec<_> = quiet.consumed().par_iter().copied().collect();
            (CoreFaunaDraft::Quiet(quiet), consumed)
        } else {
            let complete = owner
                .advance_with_hunts(&patches, &[], &exposure, hunted, self.core.seed, tick)
                .map_err(str::to_owned)?;
            (CoreFaunaDraft::Complete(complete.fauna), complete.consumed)
        };
        if !consumed.is_empty() {
            let resources = gathering
                .as_mut()
                .ok_or("grazing requires resource owner")?;
            resources
                .nodes
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
        Ok(Some(draft))
    }

    pub(super) fn publish_fauna(&mut self, draft: Option<CoreFaunaDraft>) {
        match draft {
            Some(CoreFaunaDraft::Quiet(draft)) => self
                .fauna
                .as_mut()
                .expect("staged fauna")
                .publish_quiet_validated(draft),
            Some(CoreFaunaDraft::Complete(fauna)) => self.fauna = Some(fauna),
            None => {}
        }
    }
}
