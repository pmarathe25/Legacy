//! Formal verse and musical phrasing operate on the composer's supplied words.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Genre {
    Poem,
    Song,
    Book,
    Story,
}
pub struct Phrasing {
    pub line_ends: Vec<usize>,
    pub notes: Vec<i32>,
}
pub fn arrange(genre: &Genre, sounds: &mut Vec<Vec<u8>>, gloss: &mut Vec<String>) -> Phrasing {
    let width = sounds.len();
    let line_ends = match genre {
        Genre::Poem | Genre::Song if width >= 2 => {
            sounds.extend([sounds[0].clone(), sounds[1].clone(), sounds[0].clone()]);
            gloss.extend([gloss[0].clone(), gloss[1].clone(), gloss[0].clone()]);
            vec![width, sounds.len()]
        }
        _ => vec![width],
    };
    let notes = if *genre == Genre::Song {
        sounds
            .iter()
            .map(|w| (w.iter().map(|p| i32::from(*p)).sum::<i32>() % 12) + 48)
            .collect()
    } else {
        vec![]
    };
    Phrasing { line_ends, notes }
}
