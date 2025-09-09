use crate::{Gloss, Sense, WordEntryComplete};
use itertools::Itertools;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Sound {
    pub ipa: Option<String>,
}
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct WordEntry {
    pub pos: Option<String>,
    pub lang_code: Option<String>,
    pub word: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Hyphenation {
    pub parts: Vec<String>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct KaikkiWordEntry {
    pub pos: Option<String>,
    pub word: String,
    pub senses: Vec<KaikkiSense>,
    #[serde(default)]
    pub hyphenations: Vec<Hyphenation>,
    #[serde(default)]
    pub sounds: Vec<Sound>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct KaikkiSense {
    #[serde(default)]
    pub glosses: Vec<String>,
}

impl KaikkiWordEntry {
    pub fn to_word_entry_complete(self) -> (WordEntryComplete, Vec<Sound>, Vec<Hyphenation>) {
        let pos = self.pos.unwrap_or_else(|| "unknown".to_string());
        let senses = self
            .senses
            .into_iter()
            .map(|kaikki_sense| Sense {
                pos: pos.clone(),
                glosses: vec![Gloss {
                    gloss_lines: kaikki_sense
                        .glosses
                        .iter()
                        .map(|s| s.as_str().trim().trim_end_matches(".").to_string())
                        .unique()
                        .collect(),
                }]
                .iter()
                .cloned()
                .unique()
                .collect(),
            })
            .collect();

        (WordEntryComplete { senses }, self.sounds, self.hyphenations)
    }
}
