//! Learned relational constructions. Words, observations, and bodily ability are supplied.
//! There is no inherited word order, universal role marker, or generation gate.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub type Sound = Vec<u8>;
/// Semantic roles describe a perceived event, not the words used to express it.
/// Dot-separated roles may embed another clause (e.g. patient.@predicate).
pub type Scene = BTreeMap<String, String>;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub predicate: String,
    pub order: Vec<String>,
    pub markers: BTreeMap<String, Sound>,
    pub confirmations: u16,
    pub contrasts: Vec<u64>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grammar {
    pub rules: Vec<Rule>,
    pub ambiguities: u16,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Utterance {
    pub words: Vec<Sound>,
}
fn shape(scene: &Scene) -> Vec<String> {
    scene.keys().cloned().collect()
}
fn same_shape(rule: &Rule, scene: &Scene) -> bool {
    rule.predicate == scene["@predicate"]
        && rule.order.len() == scene.len()
        && rule.order.iter().all(|k| scene.contains_key(k))
}
impl Rule {
    fn productive(&self) -> bool {
        self.confirmations >= 4 && self.contrasts.len() >= 2
    }
}
impl Grammar {
    pub fn valid(&self, phonemes: usize) -> bool {
        self.rules.len() <= 64
            && self.rules.iter().all(|r| {
                !r.predicate.is_empty()
                    && r.predicate.len() <= 160
                    && (2..=24).contains(&r.order.len())
                    && r.confirmations > 0
                    && r.confirmations <= 1000
                    && r.order.iter().collect::<BTreeSet<_>>().len() == r.order.len()
                    && r.order.contains(&"@predicate".into())
                    && r.order
                        .iter()
                        .all(|s| !s.is_empty() && s.len() <= 128 && s.split('.').count() <= 4)
                    && r.markers.iter().all(|(k, s)| {
                        r.order.contains(k)
                            && !s.is_empty()
                            && s.len() <= 16
                            && s.iter().all(|p| usize::from(*p) < phonemes)
                    })
                    && r.contrasts.len() <= 8
                    && !r.contrasts.is_empty()
                    && r.contrasts.iter().collect::<BTreeSet<_>>().len() == r.contrasts.len()
            })
    }
    pub fn productive_rules(&self) -> usize {
        self.rules.iter().filter(|r| r.productive()).count()
    }
    /// More involved innovations require established constructions, ambiguity, and cognition.
    pub fn can_mark_roles(&self, capacity: u64) -> bool {
        capacity >= 800
            && self.ambiguities >= 3
            && self
                .rules
                .iter()
                .any(|r| r.productive() && r.confirmations >= 12)
    }
    pub fn can_embed(&self, capacity: u64) -> bool {
        capacity >= 1600
            && self.rules.iter().filter(|r| r.productive()).count() >= 2
            && self
                .rules
                .iter()
                .map(|r| u64::from(r.confirmations))
                .sum::<u64>()
                >= 64
    }
    /// A remembered convention is preferred; otherwise the caller supplies a personal order.
    pub fn express(
        &self,
        scene: &Scene,
        order: &[String],
        markers: &BTreeMap<String, Sound>,
        mut word: impl FnMut(&str) -> Sound,
    ) -> Option<Utterance> {
        if !valid_scene(scene) {
            return None;
        }
        let rule = self
            .rules
            .iter()
            .filter(|r| markers.is_empty() && same_shape(r, scene) && r.productive())
            .max_by_key(|r| r.confirmations);
        let (order, markers) = rule.map_or((order, markers), |r| (r.order.as_slice(), &r.markers));
        if order.len() != scene.len() || order.iter().collect::<BTreeSet<_>>().len() != scene.len()
        {
            return None;
        }
        let mut words = vec![];
        for slot in order {
            if let Some(marker) = markers.get(slot) {
                words.push(marker.clone());
            }
            let sounds = word(scene.get(slot)?);
            if sounds.is_empty() {
                return None;
            }
            words.push(sounds);
        }
        Some(Utterance { words })
    }
    /// Interpret solely with the listener's learned constructions and word meanings.
    /// Conflicting parses remain ambiguous rather than choosing the author's intention.
    pub fn interpret(
        &self,
        utterance: &Utterance,
        mut meaning: impl FnMut(&[u8]) -> Option<String>,
    ) -> Option<Scene> {
        let mut parses = BTreeSet::new();
        for rule in self.rules.iter().filter(|r| r.productive()) {
            let mut cursor = 0;
            let mut scene = Scene::new();
            let mut ok = true;
            for slot in &rule.order {
                if let Some(marker) = rule.markers.get(slot) {
                    if utterance.words.get(cursor) != Some(marker) {
                        ok = false;
                        break;
                    }
                    cursor += 1;
                }
                let Some(value) = utterance.words.get(cursor).and_then(|w| meaning(w)) else {
                    ok = false;
                    break;
                };
                scene.insert(slot.clone(), value);
                cursor += 1;
            }
            if ok
                && cursor == utterance.words.len()
                && valid_scene(&scene)
                && scene["@predicate"] == rule.predicate
            {
                parses.insert(scene);
            }
        }
        if parses.len() == 1 {
            parses.pop_first()
        } else {
            None
        }
    }
    /// Infer order and particles only from a scene independently observed by the learner.
    /// An unheard intended gloss is never accepted as lexical evidence.
    pub fn observe(
        &mut self,
        utterance: &Utterance,
        scene: &Scene,
        capacity: u64,
        mut meaning: impl FnMut(&[u8]) -> Option<String>,
    ) -> bool {
        if capacity < 300 || !valid_scene(scene) || utterance.words.len() > 48 {
            return false;
        }
        let mut order = vec![];
        let mut markers = BTreeMap::new();
        let mut pending = None;
        for sounds in &utterance.words {
            let decoded = meaning(sounds);
            let roles: Vec<_> = scene
                .iter()
                .filter(|(_, value)| decoded.as_ref() == Some(*value))
                .map(|(k, _)| k)
                .collect();
            if roles.len() == 1 && !order.contains(roles[0]) {
                let role = roles[0].clone();
                if let Some(marker) = pending.take() {
                    markers.insert(role.clone(), marker);
                }
                order.push(role);
            } else if roles.is_empty() && pending.is_none() && capacity >= 500 {
                pending = Some(sounds.clone());
            } else {
                self.ambiguities = self.ambiguities.saturating_add(1);
                return false;
            }
        }
        if order.len() != scene.len() || pending.is_some() {
            return false;
        }
        if scene.keys().any(|s| s.contains('.')) && capacity < 800 {
            return false;
        }
        self.reinforce(Rule {
            predicate: scene["@predicate"].clone(),
            order,
            markers,
            confirmations: 1,
            contrasts: vec![fingerprint(scene)],
        });
        true
    }
    /// A speaker can retain a successful personal experiment, never their genes' contents.
    pub fn confirm(&mut self, scene: &Scene, order: Vec<String>, markers: BTreeMap<String, Sound>) {
        if valid_scene(scene)
            && order.len() == scene.len()
            && order.iter().collect::<BTreeSet<_>>().len() == order.len()
            && order.iter().all(|s| scene.contains_key(s))
            && markers
                .iter()
                .all(|(s, w)| order.contains(s) && !w.is_empty() && w.len() <= 16)
        {
            self.reinforce(Rule {
                predicate: scene["@predicate"].clone(),
                order,
                markers,
                confirmations: 1,
                contrasts: vec![fingerprint(scene)],
            });
        }
    }
    pub fn failed(&mut self) {
        self.ambiguities = self.ambiguities.saturating_add(1);
    }
    fn reinforce(&mut self, mut rule: Rule) {
        if let Some(old) = self.rules.iter_mut().find(|r| {
            r.predicate == rule.predicate && r.order == rule.order && r.markers == rule.markers
        }) {
            old.confirmations = old.confirmations.saturating_add(1).min(1000);
            let scene = rule.contrasts.pop().unwrap();
            if !old.contrasts.contains(&scene) && old.contrasts.len() < 8 {
                old.contrasts.push(scene);
            }
        } else {
            if self.rules.len() == 64 {
                let i = self
                    .rules
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, r)| r.confirmations)
                    .unwrap()
                    .0;
                self.rules.remove(i);
            }
            self.rules.push(rule);
        }
    }
    pub fn role_order(scene: &Scene, seed: u64) -> Vec<String> {
        let mut order = shape(scene);
        // Supplied seed varies personal experiments; no favored order is universal.
        for i in (1..order.len()).rev() {
            order.swap(i, (seed.rotate_right(i as u32 * 7) as usize) % (i + 1));
        }
        order
    }
}
pub fn valid_scene(scene: &Scene) -> bool {
    (2..=24).contains(&scene.len())
        && scene.contains_key("@predicate")
        && scene.iter().all(|(key, value)| {
            !key.is_empty()
                && key.len() <= 128
                && !value.is_empty()
                && value.len() <= 160
                && key.split('.').count() <= 4
        })
}

fn fingerprint(scene: &Scene) -> u64 {
    let mut hash = 14695981039346656037u64;
    for (k, v) in scene {
        for text in [k, v] {
            for b in text.bytes() {
                hash = (hash ^ u64::from(b)).wrapping_mul(1099511628211);
            }
            hash = (hash ^ 255).wrapping_mul(1099511628211);
        }
    }
    hash
}
