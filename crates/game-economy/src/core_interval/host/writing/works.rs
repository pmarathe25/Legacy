//! Compose and interpret a page through personal phonemes and marks.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreWorkInscription {
    pub person: u64,
    pub words: Vec<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreWorkObservation {
    pub item: u64,
    pub surface: Option<u32>,
    pub forms: Vec<CoreMarkObservation>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct CoreWorkEdition {
    pub person: u64,
    pub item: u64,
    pub at_millis: u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub surface: Option<u32>,
}

impl CoreHost {
    pub fn inscribe_work(
        &mut self,
        choices: &[CoreWorkInscription],
    ) -> Result<Vec<(u64, Option<u64>)>, String> {
        if choices.is_empty() {
            return Ok(Vec::new());
        }
        if choices
            .par_windows(2)
            .any(|pair| pair[0].person >= pair[1].person)
        {
            return Err("invalid composition choices".into());
        }
        let forms = self.personal_glyph_forms()?;
        let pages = choices
            .par_iter()
            .map(|choice| {
                if !(2..=crate::writing::MAX_WORK_FORMS).contains(&choice.words.len()) {
                    return Err("a work requires between two and sixteen marked forms");
                }
                let page = work_marks(forms, choice.person, &choice.words)
                    .ok_or("writer has no chosen marks for a term in the work")?;
                Ok((choice.person, page))
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.write_glyph_pages(pages)
    }

    pub fn observe_written_work(
        &self,
        person: u64,
        item: u64,
    ) -> Result<CoreWorkObservation, String> {
        let page = self.held_written_page(person, item)?;
        let work = page.work.as_ref().ok_or("page has no composed work")?;
        let forms = work
            .forms
            .iter()
            .map(|marks| self.interpret_marks(person, marks))
            .collect();
        Ok(CoreWorkObservation { item, surface: page.surface, forms })
    }

    pub(in crate::core_interval::host) fn compose_autonomously(
        &mut self,
        blocked: &[u64],
    ) -> Result<Vec<CoreWorkEdition>, String> {
        let Some(owner) = self
            .writing
            .as_ref()
            .filter(|owner| !owner.compositions.is_empty())
        else {
            return Ok(Vec::new());
        };
        let forms = self.personal_glyph_forms()?;
        let language = self
            .social
            .as_ref()
            .and_then(|owner| owner.language.as_ref())
            .expect("writing forms require personal language");
        let busy = self.writing_busy(blocked);
        let now = self.core.simulated_millis();
        let pages = owner
            .compositions
            .par_iter()
            .filter_map(|intent| {
                if intent.completed
                    || intent.value <= intent.idle
                    || busy.binary_search(&intent.person).is_ok()
                {
                    return None;
                }
                let actor = self.core.person(intent.person)?;
                if actor.controlled || actor.health == 0 || actor.region.is_none() {
                    return None;
                }
                if self
                    .pastimes
                    .as_ref()
                    .is_some_and(|pastimes| pastimes.session(actor.id).is_some())
                {
                    return None;
                }
                let competing_value = self.pastimes.as_ref().and_then(|pastimes| {
                    pastimes.preferred_value(actor.id, now, |activity| {
                        self.pastime_ready(actor.id, activity, true)
                    })
                });
                if competing_value.is_some_and(|value| value >= intent.value) {
                    return None;
                }
                let glyphs = if intent.topics.is_empty() {
                    work_marks(forms, intent.person, &intent.words)?
                } else {
                    let index = language
                        .minds
                        .binary_search_by_key(&intent.person, |row| row.person)
                        .ok()?;
                    let words = &language.minds[index].words;
                    let marked = intent
                        .topics
                        .iter()
                        .map(|meaning| {
                            let index = words
                                .binary_search_by_key(meaning, |word| word.meaning)
                                .ok()?;
                            forms
                                .marks(intent.person, &words[index].sounds)
                                .map(<[u32]>::to_vec)
                        })
                        .collect::<Option<Vec<_>>>()?;
                    GlyphPage::Work(marked)
                };
                Some((intent.person, glyphs))
            })
            .collect::<Vec<_>>();
        let results = self.write_glyph_pages(pages)?;
        let writing = self.writing.as_mut().expect("composition owner exists");
        let receipts: Vec<_> = results
            .into_iter()
            .filter_map(|(person, item)| {
                let item = item?;
                Some(CoreWorkEdition {
                    person,
                    item,
                    at_millis: now,
                    surface: writing.page(item).expect("accepted edition exists").surface,
                })
            })
            .collect();
        if !receipts.is_empty() {
            Arc::make_mut(&mut writing.compositions)
                .par_iter_mut()
                .for_each(|intent| {
                    if receipts
                        .binary_search_by_key(&intent.person, |row| row.person)
                        .is_ok()
                    {
                        intent.completed = true;
                    }
                });
        }
        Ok(receipts)
    }
}

fn work_marks(
    forms: &game_language::writing::Writing,
    person: u64,
    words: &[Vec<u8>],
) -> Option<GlyphPage> {
    words
        .iter()
        .map(|sounds| forms.marks(person, sounds).map(<[u32]>::to_vec))
        .collect::<Option<Vec<_>>>()
        .map(GlyphPage::Work)
}
