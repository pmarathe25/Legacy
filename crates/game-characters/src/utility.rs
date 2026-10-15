use super::*;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Candidate {
    pub id: u64,
    pub need: i64,
    pub goal: i64,
    pub value: i64,
    pub effort: i64,
    pub risk: i64,
    pub survival: bool,
    pub reasons: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    pub id: u64,
    pub score: i64,
    pub reasons: Vec<String>,
}
impl Candidate {
    pub fn score(&self) -> i64 {
        self.need
            .saturating_add(self.goal)
            .saturating_add(self.value)
            .saturating_sub(self.effort)
            .saturating_sub(self.risk)
    }
}
pub fn choose(
    candidates: &[Candidate],
    current: Option<u64>,
    margin: i64,
    urgent: bool,
) -> Option<Decision> {
    let viable = |c: &&Candidate| !urgent || c.survival;
    let best = candidates
        .iter()
        .filter(viable)
        .max_by_key(|c| (c.score(), std::cmp::Reverse(c.id)))?;
    let chosen = if !urgent {
        candidates
            .iter()
            .find(|c| Some(c.id) == current && best.score() <= c.score().saturating_add(margin))
            .unwrap_or(best)
    } else {
        best
    };
    Some(Decision {
        id: chosen.id,
        score: chosen.score(),
        reasons: chosen.reasons.clone(),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn urgent_survival_beats_ambition_and_commitment_avoids_oscillation() {
        let a = Candidate {
            id: 1,
            need: 20,
            goal: 0,
            value: 0,
            effort: 0,
            risk: 0,
            survival: true,
            reasons: vec![],
        };
        let b = Candidate {
            id: 2,
            need: 0,
            goal: 100,
            value: 0,
            effort: 0,
            risk: 0,
            survival: false,
            reasons: vec![],
        };
        assert_eq!(choose(&[a.clone(), b], None, 5, true).unwrap().id, 1);
        let mut better = a.clone();
        better.id = 2;
        better.goal = 4;
        assert_eq!(choose(&[a, better], Some(1), 5, false).unwrap().id, 1);
    }
}
