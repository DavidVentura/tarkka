use crate::{FormOf, Gloss, Hyphenation, Sense, Sound, WordEntryComplete};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct WordEntry {
    pub pos: Option<String>,
    pub lang_code: Option<String>,
    pub word: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
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
    pub form_of: Vec<FormOf>,
    #[serde(default)]
    pub glosses: Vec<String>,
}

impl KaikkiWordEntry {
    pub fn to_word_entry_complete(self) -> WordEntryComplete {
        let pos = self.pos.unwrap_or_else(|| "unknown".to_string());
        let senses = self
            .senses
            .into_iter()
            .map(|kaikki_sense| {
                let glosses = kaikki_sense.glosses;
                let glosses = if glosses.is_empty() {
                    vec![]
                } else if glosses.len() == 1 {
                    // Single gloss, no categories
                    vec![Gloss {
                        shared_prefix_count: 0,
                        new_categories: vec![],
                        gloss: glosses[0].clone(),
                    }]
                } else {
                    // Multiple glosses: all but last are categories, last is the actual gloss
                    let categories = glosses[..glosses.len() - 1].to_vec();
                    let gloss_text = glosses[glosses.len() - 1].clone();
                    vec![Gloss {
                        shared_prefix_count: 0,
                        new_categories: categories,
                        gloss: gloss_text,
                    }]
                };
                Sense {
                    pos: pos.clone(),
                    form_of: kaikki_sense.form_of,
                    glosses,
                }
            })
            .collect();

        WordEntryComplete {
            senses,
            hyphenations: self.hyphenations,
            sounds: self.sounds,
        }
    }
}
