//! Numeric sounds and reversible IAST-based player transcription.
//! This display alphabet is not a writing system known by simulated characters.
pub const PHONEMES: &[&str] = &[
    "a", "ā", "i", "ī", "u", "ū", "ṛ", "e", "ai", "o", "au", "k", "kh", "g", "gh", "ṅ", "c", "j",
    "ñ", "ṭ", "ḍ", "ṇ", "t", "th", "d", "dh", "n", "p", "ph", "b", "bh", "m", "y", "r", "l", "v",
    "ś", "ṣ", "s", "h", "f", "w",
];
pub const VOWELS: &[u8] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
pub fn vowel(id: u8) -> bool {
    VOWELS.contains(&id)
}
pub fn valid(sounds: &[u8]) -> bool {
    !sounds.is_empty() && sounds.iter().all(|p| usize::from(*p) < PHONEMES.len())
}
/// Insert a boundary only where concatenation would denote a different phoneme.
/// Thus k·h and kh differ, as do a·i and the diphthong ai.
pub fn render(sounds: &[u8]) -> String {
    let mut result = String::new();
    for (i, id) in sounds.iter().enumerate() {
        let Some(sound) = PHONEMES.get(usize::from(*id)) else {
            result.push('�');
            continue;
        };
        if i > 0 {
            let previous = PHONEMES
                .get(usize::from(sounds[i - 1]))
                .copied()
                .unwrap_or("");
            if PHONEMES.iter().any(|p| {
                p.len() == previous.len() + sound.len()
                    && p.starts_with(previous)
                    && p.ends_with(sound)
            }) {
                result.push('·');
            }
        }
        result.push_str(sound);
    }
    result
}
/// Longest-match parsing; a middle dot explicitly separates adjacent sounds.
pub fn parse(text: &str) -> Option<Vec<u8>> {
    let mut rest = text;
    let mut sounds = vec![];
    let mut boundary = false;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix('·') {
            if sounds.is_empty() || boundary || after.is_empty() {
                return None;
            }
            boundary = true;
            rest = after;
            continue;
        }
        let found = PHONEMES
            .iter()
            .enumerate()
            .filter(|(_, p)| rest.starts_with(**p))
            .max_by_key(|(_, p)| p.len())
            .map(|(i, p)| (i as u8, p.len()));
        let (id, width) = found?;
        sounds.push(id);
        rest = &rest[width..];
        boundary = false;
    }
    (!sounds.is_empty()).then_some(sounds)
}
