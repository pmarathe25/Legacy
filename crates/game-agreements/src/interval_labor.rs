//! Elapsed labor selection and guarded publication from factual work state.
use crate::*;
use game_ecs::{ReadFootprint, ReadGuard, ReadSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntervalActivity {
    Work { site: SiteId },
    Cultivate { field: SiteId },
    Other,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntervalLaborFacts {
    pub actor: PersonId,
    pub activity: IntervalActivity,
    pub tool_ready: bool,
    pub construction: Option<ProjectId>,
}
fn matches_assignment(row: &Obligation, assignments: &[WorkAssignment]) -> bool {
    assignments.iter().any(|assignment| match row.term {
        AgreementTerm::Labor { debtor, site, .. } => {
            debtor == assignment.actor && assignment.target == WorkTarget::Site(site)
        }
        AgreementTerm::ConstructionLabor {
            debtor, project, ..
        } => debtor == assignment.actor && assignment.target == WorkTarget::Project(project),
        _ => false,
    })
}
impl Ledger<'_> {
    /// Work and cultivation require their equipment fact. Construction credit
    /// follows the current construction row. These interval rules do not test
    /// vitality: admission and interruption remain the relevant action owners.
    pub fn prepare_interval_labor(
        &self,
        from: SimTime,
        to: SimTime,
        physical_enabled: bool,
        projects_enabled: bool,
        facts: impl IntoIterator<Item = IntervalLaborFacts>,
    ) -> Result<PreparedLabor, Error> {
        let mut actors = BTreeSet::new();
        let mut assignments = Vec::new();
        for fact in facts {
            if !actors.insert(fact.actor) {
                return Err(Error::Unavailable);
            }
            if physical_enabled && fact.tool_ready {
                let site = match fact.activity {
                    IntervalActivity::Work { site } => Some(site),
                    IntervalActivity::Cultivate { field } => Some(field),
                    IntervalActivity::Other => None,
                };
                if let Some(site) = site {
                    assignments.push(WorkAssignment {
                        actor: fact.actor,
                        target: WorkTarget::Site(site),
                    });
                }
            }
            if projects_enabled && let Some(project) = fact.construction {
                assignments.push(WorkAssignment {
                    actor: fact.actor,
                    target: WorkTarget::Project(project),
                });
            }
        }
        self.prepare_labor_selected(from, to, &assignments, true)
    }
}
impl PreparedLabor {
    pub fn stage_into(&self, store: &Store, batch: &mut Batch) -> Result<ReadGuard, Error> {
        let members = obligations(store)
            .filter(|(_, row)| matches_assignment(row, &self.assignments))
            .map(|(id, _)| id)
            .collect::<BTreeSet<_>>();
        if members != self.obligation_members {
            return Err(Error::Unavailable);
        }
        let mut reads = ReadSet::new();
        let assignments = self.assignments.clone();
        reads.membership::<Obligation>(store, move |_, row| matches_assignment(row, &assignments));
        for (id, old) in &self.agreements_before {
            reads.expect_row(StableId(id.0), Some(old.clone()));
        }
        for (id, old) in &self.read_obligations {
            reads.expect_row(StableId(id.0), Some(old.clone()));
        }
        for (id, old) in &self.pending_before {
            reads.expect_row(StableId(id.0), old.clone());
        }
        reads.validate(store)?;
        let mut admitted = ReadFootprint::new();
        for patch in &self.patches {
            let id = StableId(patch.after.id.0);
            batch.insert(id, patch.after.clone());
            admitted.protect::<Obligation>(id);
        }
        for (id, (_, millis)) in &self.pending {
            batch.insert(StableId(id.0), PendingWork { millis: *millis });
            admitted.protect::<PendingWork>(StableId(id.0));
        }
        for (id, status) in &self.statuses {
            let mut after = self
                .agreements_before
                .get(id)
                .ok_or(Error::Unavailable)?
                .clone();
            after.status = *status;
            batch.insert(StableId(id.0), after);
            admitted.protect::<Agreement>(StableId(id.0));
        }
        reads.bind(store, batch, &admitted)
    }
}
