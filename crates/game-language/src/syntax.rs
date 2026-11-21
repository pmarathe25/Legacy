//! Personal two-word constructions learned during physical demonstrations.
use super::{Event, Language, Meaning, random};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Order {
    HolderFirst,
    ObjectFirst,
}

impl Order {
    fn reversed(self) -> Self {
        match self {
            Self::HolderFirst => Self::ObjectFirst,
            Self::ObjectFirst => Self::HolderFirst,
        }
    }

    fn arrange<T>(self, holder: T, object: T) -> [T; 2] {
        match self {
            Self::HolderFirst => [holder, object],
            Self::ObjectFirst => [object, holder],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Holdings {
    pub holder: u64,
    pub object: u32,
}

impl Holdings {
    fn valid(self, people: &HashSet<u64>) -> bool {
        self.object != 0 && people.contains(&self.holder)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Example {
    order: Order,
    scene: Holdings,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mind {
    /// Chances per thousand of choosing a construction or attending to an example.
    #[serde(default)]
    pub initiative: u16,
    #[serde(default)]
    pub inference: u16,
    #[serde(default)]
    pub demonstrate: bool,
    #[serde(default)]
    pub reverse_on_adoption: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<Order>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    example: Option<Example>,
}

impl Mind {
    fn from_inclinations(mind: &super::Mind) -> Self {
        Self {
            initiative: mind.initiative,
            inference: mind.adoption,
            demonstrate: mind.learn,
            reverse_on_adoption: mind.modify,
            ..Self::default()
        }
    }

    pub(super) fn valid(&self) -> bool {
        self.initiative <= 1000
            && self.inference <= 1000
            && self.example.is_none_or(|example| {
                self.order.is_none() && example.scene.holder != 0 && example.scene.object != 0
            })
    }

    pub(super) fn knows_examples(&self, people: &HashSet<u64>) -> bool {
        self.example
            .is_none_or(|example| example.scene.valid(people))
    }

    fn consider(&mut self, order: Order, scene: Holdings) -> Option<Order> {
        if self.order.is_some() {
            return None;
        }
        if self
            .example
            .is_some_and(|example| example.order == order && example.scene != scene)
        {
            let adopted = if self.reverse_on_adoption {
                order.reversed()
            } else {
                order
            };
            self.order = Some(adopted);
            self.example = None;
            return Some(adopted);
        }
        self.example = Some(Example { order, scene });
        None
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Clause {
    pub parts: [Vec<u8>; 2],
    pub intended: Holdings,
    pub demonstrated: bool,
    pub interpreted: Option<Holdings>,
    pub adopted_order: Option<Order>,
}

impl Clause {
    pub(super) fn valid(&self, event: &Event, people: &HashSet<u64>, sounds: usize) -> bool {
        self.intended.holder == event.speaker
            && event.meaning == Meaning::Object(self.intended.object)
            && self.parts.contains(&event.sounds)
            && self.parts.iter().all(|part| {
                !part.is_empty()
                    && part.len() <= 16
                    && part.iter().all(|s| usize::from(*s) < sounds)
            })
            && self
                .interpreted
                .is_none_or(|scene| event.heard && scene.valid(people))
            && self
                .adopted_order
                .is_none_or(|_| event.heard && self.demonstrated)
    }
}

fn decode(mind: &super::Mind, sounds: &[u8]) -> Option<Meaning> {
    let mut meanings = mind.words.iter().filter(|word| word.sounds == sounds);
    let result = meanings.next()?.meaning;
    meanings.next().is_none().then_some(result)
}

fn reading(mind: &super::Mind, parts: &[Vec<u8>; 2], order: Order) -> Option<Holdings> {
    let [holder, object] = order.arrange(&parts[0], &parts[1]);
    match (decode(mind, holder)?, decode(mind, object)?) {
        (Meaning::Person(holder), Meaning::Object(object)) => Some(Holdings { holder, object }),
        _ => None,
    }
}

/// Object topics come from the speaker deliberately showing an object in their
/// physical custody. Assertions without a physical scene cannot use this path.
pub(super) fn extend_grounded_objects(opening: &Language, next: &mut Language, seed: u64) {
    if !opening.minds.par_iter().any(|mind| {
        mind.syntax
            .as_ref()
            .is_some_and(|syntax| syntax.initiative > 0)
    }) {
        return;
    }
    let mind = |id| {
        opening
            .minds
            .binary_search_by_key(&id, |mind| mind.person)
            .ok()
            .map(|i| &opening.minds[i])
    };
    let mut changes: Vec<_> = next
        .events
        .par_iter_mut()
        .flat_map_iter(|event| {
            let mut changes = [None, None];
            let mut extend = || -> Option<()> {
                let Meaning::Object(object) = event.meaning else {
                    return None;
                };
                let source = mind(event.speaker)?;
                let mut source_syntax = source.syntax.clone()?;
                let chance =
                    random(seed ^ event.tick.rotate_left(9) ^ event.speaker.rotate_left(27));
                if chance % 1000 >= u64::from(source_syntax.initiative) {
                    return None;
                }
                let known_holder = source
                    .words
                    .binary_search_by_key(&Meaning::Person(event.speaker), |word| word.meaning)
                    .ok()?;
                let order = source_syntax.order.unwrap_or_else(|| {
                    if random(seed ^ event.speaker.rotate_left(43)) & 1 == 0 {
                        Order::HolderFirst
                    } else {
                        Order::ObjectFirst
                    }
                });
                if source_syntax.order.is_none() {
                    source_syntax.order = Some(order);
                    source_syntax.example = None;
                    changes[0] = Some((event.speaker, source_syntax.clone()));
                }
                let parts = order.arrange(
                    source.words[known_holder].sounds.clone(),
                    event.sounds.clone(),
                );
                let scene = Holdings {
                    holder: event.speaker,
                    object,
                };
                let mut clause = Clause {
                    parts,
                    intended: scene,
                    demonstrated: source_syntax.demonstrate,
                    interpreted: None,
                    adopted_order: None,
                };
                if event.heard
                    && let Some(target) = mind(event.listener)
                {
                    let mut syntax = target
                        .syntax
                        .clone()
                        .unwrap_or_else(|| Mind::from_inclinations(target));
                    clause.interpreted = syntax
                        .order
                        .and_then(|order| reading(target, &clause.parts, order));
                    if clause.demonstrated
                        && target.learn
                        && syntax.order.is_none()
                        && random(chance ^ event.listener.rotate_left(15)) % 1000
                            < u64::from(syntax.inference)
                    {
                        // Infer from independently known words and the visible holder.
                        let candidate = [Order::HolderFirst, Order::ObjectFirst]
                            .into_iter()
                            .find(|order| reading(target, &clause.parts, *order) == Some(scene));
                        if let Some(candidate) = candidate {
                            clause.adopted_order = syntax.consider(candidate, scene);
                            changes[1] = Some((event.listener, syntax));
                        }
                    }
                }
                event.clause = Some(clause);
                Some(())
            };
            extend();
            changes.into_iter().flatten()
        })
        .collect();
    if !changes.is_empty() {
        changes.par_sort_unstable_by_key(|row| row.0);
        next.minds.par_iter_mut().for_each(|mind| {
            if let Ok(index) = changes.binary_search_by_key(&mind.person, |row| row.0) {
                mind.syntax = Some(changes[index].1.clone());
            }
        });
    }
}
