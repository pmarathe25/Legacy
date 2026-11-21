//! Archive changes in the proposals and judgments held by individual people.
use game_practices::{Belief, Decision, Practices};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    AidProposed,
    WorkProposed,
    AidResponse,
    WorkResponse,
    ReadinessStatement,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
pub(super) struct Fact {
    kind: Kind,
    actor: u64,
    other: u64,
    sequence: u64,
    source_tick: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    good: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    quantity: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    recipe: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    decision: Option<Decision>,
    #[serde(skip_serializing_if = "Option::is_none")]
    approved: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    claimed_ready: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    belief: Option<Belief>,
}

type Key = (Kind, u64, u64, u64, u64);

impl Fact {
    fn activity(kind: Kind, actor: u64, other: u64, sequence: u64, source_tick: u64) -> Self {
        Self {
            kind,
            actor,
            other,
            sequence,
            source_tick,
            good: None,
            quantity: None,
            recipe: None,
            decision: None,
            approved: None,
            claimed_ready: None,
            belief: None,
        }
    }

    fn terms(mut self, good: u32, quantity: u64, recipe: Option<u32>) -> Self {
        self.good = Some(good);
        self.quantity = Some(quantity);
        self.recipe = recipe;
        self
    }

    fn response(mut self, decision: Decision, approved: bool) -> Self {
        self.decision = Some(decision);
        self.approved = Some(approved);
        self
    }

    fn key(self) -> Key {
        (
            self.kind,
            self.actor,
            self.other,
            self.sequence,
            self.source_tick,
        )
    }
}

#[derive(Default)]
pub(super) struct PracticeLog {
    visible: BTreeMap<Key, Fact>,
}

impl PracticeLog {
    pub(super) fn observe(&mut self, owner: Option<&Practices>) -> Vec<Fact> {
        let mut visible = BTreeMap::new();
        let mut add = |fact: Fact| {
            visible.insert(fact.key(), fact);
        };
        for mind in owner.into_iter().flat_map(|owner| &owner.minds) {
            for proposal in mind.authored.iter() {
                add(Fact::activity(
                    Kind::AidProposed,
                    proposal.author,
                    proposal.recipient,
                    proposal.sequence,
                    proposal.created,
                )
                .terms(proposal.good, proposal.quantity, None));
            }
            for answer in mind.inbox.iter() {
                let proposal = &answer.proposal;
                add(Fact::activity(
                    Kind::AidResponse,
                    mind.person,
                    proposal.author,
                    proposal.sequence,
                    proposal.created,
                )
                .terms(proposal.good, proposal.quantity, None)
                .response(answer.decision, answer.approved));
            }
            for report in mind.reports.iter() {
                let statement = &report.statement;
                let mut fact = Fact::activity(
                    Kind::ReadinessStatement,
                    mind.person,
                    statement.speaker,
                    statement.sequence,
                    statement.tick,
                );
                fact.claimed_ready = Some(statement.claimed_ready);
                fact.belief = Some(report.belief);
                add(fact);
            }
            if let Some(work) = &mind.work {
                if let Some(proposal) = &work.authored {
                    add(Fact::activity(
                        Kind::WorkProposed,
                        proposal.author,
                        proposal.worker,
                        proposal.sequence,
                        proposal.created,
                    )
                    .terms(
                        proposal.good,
                        proposal.quantity,
                        Some(proposal.recipe),
                    ));
                }
                for answer in &work.inbox {
                    let proposal = &answer.proposal;
                    add(Fact::activity(
                        Kind::WorkResponse,
                        mind.person,
                        proposal.author,
                        proposal.sequence,
                        proposal.created,
                    )
                    .terms(proposal.good, proposal.quantity, Some(proposal.recipe))
                    .response(answer.decision, answer.approved));
                }
            }
        }
        let mut changed: Vec<_> = visible
            .iter()
            .filter_map(|(key, current)| {
                (self.visible.get(key) != Some(current)).then_some(*current)
            })
            .collect();
        changed.retain(|row| row.decision != Some(Decision::Pending));
        self.visible = visible;
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    #[test]
    fn a_receipt_preserves_an_adoption_without_private_approval() {
        let terms = json!({
            "author": 1,
            "sequence": 1,
            "worker": 2,
            "recipe": 3,
            "good": 4,
            "quantity": 5,
            "reserve": 0,
            "created": 6,
            "expires": 9
        });
        let owner = |decision| -> Practices {
            serde_json::from_value(json!({"minds": [{
                "person": 2,
                "invent": false,
                "approve": false,
                "willing": false,
                "shortage_threshold": 5,
                "minimum_shortages": 1,
                "review_interval": 2,
                "proposal_lifetime": 4,
                "work": {
                    "invent": false,
                    "approve": false,
                    "willing": false,
                    "ask_willing": false,
                    "review_interval": 2,
                    "proposal_lifetime": 4,
                    "inbox": [{
                        "proposal": terms.clone(),
                        "delivered": 6,
                        "decision": decision,
                        "approved": false
                    }]
                }
            }]}))
            .unwrap()
        };
        let mut history = PracticeLog::default();
        assert!(history.observe(Some(&owner("pending"))).is_empty());
        let after = owner("adopted");
        let recorded = history.observe(Some(&after));
        assert_eq!(recorded.len(), 1);
        let row: Value = serde_json::to_value(recorded[0]).unwrap();
        assert_eq!(row["actor"], 2);
        assert_eq!(row["other"], 1);
        assert_eq!(row["kind"], "work_response");
        assert_eq!(row["decision"], "adopted");
        assert_eq!(row["approved"], false);
        assert!(history.observe(Some(&after)).is_empty());
    }
}
