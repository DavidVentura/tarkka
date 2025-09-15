use crate::{Gloss, Sense, WordEntryComplete, WordTag, WordWithTaggedEntries};
use itertools::Itertools;

#[cfg(feature = "indexer")]
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq)]
#[cfg_attr(feature = "indexer", derive(Debug, Serialize, Deserialize))]
pub struct Sound {
    pub ipa: Option<String>,
}

#[cfg_attr(feature = "indexer", derive(Debug, Serialize, Deserialize))]
pub struct WordEntry {
    pub pos: Option<String>,
    pub lang_code: Option<String>,
    pub word: Option<String>,
}

#[derive(Clone, PartialEq)]
#[cfg_attr(feature = "indexer", derive(Debug, Serialize, Deserialize))]
pub struct Hyphenation {
    pub parts: Vec<String>,
}
#[derive(Clone)]
#[cfg_attr(feature = "indexer", derive(Debug, Serialize, Deserialize))]
pub struct KaikkiWordEntry {
    pub pos: Option<String>,
    pub word: String,
    pub senses: Vec<KaikkiSense>,
    #[cfg_attr(feature = "indexer", serde(default))]
    pub hyphenations: Vec<Hyphenation>,
    #[cfg_attr(feature = "indexer", serde(default))]
    pub sounds: Vec<Sound>,
}

#[derive(Clone)]
#[cfg_attr(feature = "indexer", derive(Debug, Serialize, Deserialize))]
pub struct KaikkiSense {
    #[cfg_attr(feature = "indexer", serde(default))]
    pub glosses: Vec<String>,
}

impl KaikkiWordEntry {
    pub fn to_word_entry_complete(self, tag: WordTag) -> WordWithTaggedEntries {
        let pos = self.pos.unwrap_or_else(|| "unknown".to_string());
        let senses = self
            .senses
            .into_iter()
            .map(|kaikki_sense| Sense {
                pos: pos.clone(),
                glosses: [Gloss {
                    gloss_lines: kaikki_sense
                        .glosses
                        .iter()
                        .filter(|s| !(s.starts_with("More information") && s.len() > 512))
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

        let hyp: Vec<Vec<String>> = self
            .hyphenations
            .iter()
            .filter_map(|h| {
                if !h.parts.is_empty() {
                    Some(h.parts.clone())
                } else {
                    None
                }
            })
            .collect();
        WordWithTaggedEntries {
            tag,
            word: self.word,
            entries: vec![WordEntryComplete { senses }],
            sounds: self
                .sounds
                .iter()
                .find(|e| e.ipa.is_some())
                .and_then(|e| e.ipa.clone()),
            hyphenations: hyp.first().cloned().unwrap_or(vec![]),
        }
    }
}
