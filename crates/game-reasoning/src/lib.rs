//! Bounded contrasts of supplied observations; no access to simulation truth.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Observation {
    pub context: BTreeMap<String, i64>,
    pub outcome: String,
    pub subject: String,
    /// Shared nonzero IDs identify deliberately split samples.
    pub trial: u64,
    /// Perceived outcome in the supplying feature's units, not a hidden probability.
    pub value: i64,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Claim {
    pub comparisons: u32,
    pub complexity: u16,
    pub confidence: u8,
    pub context: BTreeMap<String, i64>,
    pub effect: i64,
    pub factor: String,
    pub outcome: String,
    pub subject: String,
}
impl Claim {
    pub fn key(&self) -> String {
        let mut key =
            String::with_capacity(self.subject.len() + self.factor.len() + self.outcome.len() + 26);
        self.write_key(&mut key).unwrap();
        key
    }
    pub fn write_key(&self, output: &mut impl std::fmt::Write) -> std::fmt::Result {
        use std::fmt::Write;
        let mut fingerprint = game_core::Fingerprint::default();
        for (i, (key, value)) in self.context.iter().enumerate() {
            if i > 0 {
                fingerprint.write_char(',')?;
            }
            write!(fingerprint, "{key}={value}")?;
        }
        write!(
            output,
            "theory:{}:{}:{}:{:x}",
            self.subject,
            self.factor,
            self.outcome,
            fingerprint.finish()
        )
    }
    pub fn predicts(&self, context: &BTreeMap<String, i64>) -> bool {
        self.context.iter().all(|(k, v)| context.get(k) == Some(v))
    }
}
/// More attention permits more comparisons and more contextual conditions. Correlation
/// from uncontrolled observations remains tentative even with a capable reasoner.
pub fn infer(observations: &[Observation], mut budget: usize, conditions: usize) -> Vec<Claim> {
    let mut claims: BTreeMap<String, (Claim, i128, std::collections::BTreeSet<u64>, u32, u32)> =
        BTreeMap::new();
    'outer: for (i, a) in observations.iter().enumerate().rev() {
        for b in observations[..i].iter().rev() {
            if budget == 0 {
                break 'outer;
            }
            budget -= 1;
            if a.subject != b.subject
                || a.outcome != b.outcome
                || a.context.len() != b.context.len()
            {
                continue;
            }
            let differing: Vec<_> = a
                .context
                .iter()
                .filter(|(k, v)| b.context.get(*k) != Some(*v))
                .collect();
            if differing.len() != 1 || a.context.len() > conditions + 1 {
                continue;
            }
            let (factor, av) = differing[0];
            if factor.ends_with("-id") {
                continue;
            }
            let Some(bv) = b.context.get(factor) else {
                continue;
            };
            let difference = i128::from(*av) - i128::from(*bv);
            if difference == 0 {
                continue;
            }
            let effect = ((i128::from(a.value) - i128::from(b.value)) * 100 / difference)
                .clamp(-1000000, 1000000) as i64;
            let context: BTreeMap<String, i64> = a
                .context
                .iter()
                .filter(|(k, _)| *k != factor)
                .map(|(k, v)| (k.clone(), *v))
                .collect();
            let controlled = (a.trial != 0 && a.trial == b.trial).then_some(a.trial);
            let claim = Claim {
                subject: a.subject.clone(),
                factor: factor.clone(),
                outcome: a.outcome.clone(),
                complexity: (1 + context.len()) as u16,
                context,
                effect,
                confidence: 0,
                comparisons: 0,
            };
            let entry = claims.entry(claim.key()).or_insert((
                claim,
                0,
                std::collections::BTreeSet::new(),
                0,
                0,
            ));
            entry.0.comparisons = entry.0.comparisons.saturating_add(1);
            entry.1 += i128::from(effect);
            if let Some(trial) = controlled {
                entry.2.insert(trial);
            }
            entry.3 += u32::from(effect > 0);
            entry.4 += u32::from(effect < 0);
        }
    }
    claims
        .into_values()
        .map(|(mut c, sum, controlled, positive, negative)| {
            c.effect = (sum / i128::from(c.comparisons.max(1))) as i64;
            c.confidence = (10 + c.comparisons.min(10) * 2 + (controlled.len().min(6) as u32) * 10)
                .min(if !controlled.is_empty() { 90 } else { 30 }) as u8;
            if positive > 0 && negative > 0 {
                c.confidence = (u32::from(c.confidence) * positive.max(negative)
                    / (positive + negative)) as u8;
            }
            c
        })
        .collect()
}

/// Compose personally held conditional explanations. Conditions are retained, and
/// uncertain links cannot become certain merely by chaining them.
pub fn deduce(known: &[Claim], mut budget: usize, complexity: u16) -> Vec<Claim> {
    let mut result = BTreeMap::new();
    for a in known {
        for b in known {
            if budget == 0 {
                return result.into_values().collect();
            }
            budget -= 1;
            if let Some(claim) = generalize(a, b, complexity) {
                result.insert(claim.key(), claim);
                continue;
            }
            if a.subject != b.subject || a.outcome != b.factor || a.factor == b.outcome {
                continue;
            }
            let cost = a.complexity.saturating_add(b.complexity);
            if cost > complexity {
                continue;
            }
            let mut context = a.context.clone();
            if b.context
                .iter()
                .any(|(k, v)| context.get(k).is_some_and(|old| old != v))
            {
                continue;
            }
            context.extend(b.context.iter().map(|(k, v)| (k.clone(), *v)));
            let claim = Claim {
                subject: a.subject.clone(),
                factor: a.factor.clone(),
                outcome: b.outcome.clone(),
                complexity: cost,
                context,
                effect: (i128::from(a.effect) * i128::from(b.effect) / 100).clamp(-1000000, 1000000)
                    as i64,
                confidence: (u16::from(a.confidence.min(b.confidence)) * 3 / 4) as u8,
                comparisons: a.comparisons.min(b.comparisons),
            };
            result.insert(claim.key(), claim);
        }
    }
    result.into_values().collect()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prediction {
    pub value: i64,
    pub confidence: u8,
}
/// Project a measured baseline under changed, perceived conditions. One best claim
/// per factor avoids double-counting the same explanation at several complexities.
pub fn predict(
    known: &[Claim],
    baseline: &Observation,
    changes: &BTreeMap<String, i64>,
) -> Option<Prediction> {
    let mut target = baseline.context.clone();
    target.extend(changes.iter().map(|(k, v)| (k.clone(), *v)));
    let mut factors: BTreeMap<&str, (&Claim, i128)> = BTreeMap::new();
    for claim in known {
        if claim.subject != baseline.subject
            || claim.outcome != baseline.outcome
            || !claim.predicts(&baseline.context)
            || !claim.predicts(&target)
        {
            continue;
        }
        let Some((from, to)) = baseline
            .context
            .get(&claim.factor)
            .zip(target.get(&claim.factor))
        else {
            continue;
        };
        if from == to {
            continue;
        }
        let entry = factors
            .entry(&claim.factor)
            .or_insert((claim, i128::from(*to) - i128::from(*from)));
        if (claim.confidence, std::cmp::Reverse(claim.complexity))
            > (entry.0.confidence, std::cmp::Reverse(entry.0.complexity))
        {
            entry.0 = claim;
        }
    }
    if factors.is_empty() {
        return None;
    }
    let confidence = factors.values().map(|(c, _)| c.confidence).min().unwrap();
    let value = factors
        .values()
        .fold(i128::from(baseline.value), |n, (c, delta)| {
            n + i128::from(c.effect) * delta * i128::from(c.confidence) / 10000
        });
    Some(Prediction {
        value: value.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64,
        confidence,
    })
}

/// Agreement across varied settings can support an uncertain abstraction. Conditions
/// are removed only when the actor has comparable evidence on both sides of that change.
fn generalize(a: &Claim, b: &Claim, limit: u16) -> Option<Claim> {
    let cost = a.complexity.saturating_add(b.complexity).saturating_add(1);
    if cost > limit
        || a.subject != b.subject
        || a.factor != b.factor
        || a.outcome != b.outcome
        || !a.context.keys().eq(b.context.keys())
        || a.context
            .iter()
            .filter(|(k, v)| b.context.get(*k) != Some(*v))
            .count()
            != 1
        || a.effect.signum() != b.effect.signum()
        || (i128::from(a.effect) - i128::from(b.effect)).abs() * 4
            > i128::from(a.effect).abs().max(i128::from(b.effect).abs())
    {
        return None;
    }
    let mut claim = a.clone();
    claim.context.retain(|k, v| b.context.get(k) == Some(v));
    claim.complexity = cost;
    claim.effect = ((i128::from(a.effect) + i128::from(b.effect)) / 2) as i64;
    claim.confidence = (u16::from(a.confidence.min(b.confidence)) * 3 / 4) as u8;
    claim.comparisons = a.comparisons.saturating_add(b.comparisons);
    Some(claim)
}
