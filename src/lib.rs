use serde::{Deserialize, Serialize};

pub mod kaikki;
pub mod reader;

#[cfg(target_os = "android")]
pub mod android;

pub const HEADER_SIZE: u8 = 16;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WordEntryComplete {
    pub senses: Vec<Sense>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Gloss {
    pub shared_prefix_count: u8,
    pub new_categories: Vec<String>,
    pub gloss: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Sense {
    pub pos: String,
    pub glosses: Vec<Gloss>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Hyphenation {
    pub parts: Vec<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub enum WordTag {
    Monolingual = 1,
    English = 2,
    Both = 3,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WordWithTaggedEntries {
    pub word: String,
    pub tag: WordTag,
    pub entries: Vec<WordEntryComplete>,
    pub sounds: Option<String>,
    pub hyphenations: Option<Hyphenation>,
}

impl WordWithTaggedEntries {
    pub fn serialize(&self) -> Vec<u8> {
        let mut v = Vec::new();

        // Serialize tag (1 byte)
        v.push(self.tag as u8);

        // Serialize number of entries (1 byte)
        assert!(
            self.entries.len() < 256,
            "Too many entries for word: {}",
            self.word
        );
        v.push(self.entries.len() as u8);
        assert!(self.entries.len() <= 2);

        // Serialize each entry
        for entry in &self.entries {
            let serialized = serde_json::to_vec(entry).unwrap();
            // Store entry size as u16
            let size = serialized.len();
            assert!(
                size <= u16::MAX as usize,
                "Entry too large for word: {}",
                self.word
            );
            v.extend_from_slice(&(size as u16).to_le_bytes());
            v.extend_from_slice(&serialized);
        }

        // Serialize sounds (optional)
        let sounds_data = if let Some(ref sound_str) = self.sounds {
            sound_str.as_bytes().to_vec()
        } else {
            Vec::new()
        };
        v.extend_from_slice(&(sounds_data.len() as u16).to_le_bytes());
        v.extend_from_slice(&sounds_data);

        // Serialize hyphenations (optional)
        let hyphenations_data = if let Some(ref hyphenation) = self.hyphenations {
            serde_json::to_vec(hyphenation).unwrap()
        } else {
            Vec::new()
        };
        v.extend_from_slice(&(hyphenations_data.len() as u16).to_le_bytes());
        v.extend_from_slice(&hyphenations_data);

        v
    }

    pub fn deserialize(data: &[u8], word: String) -> Result<Self, &'static str> {
        let mut pos = 0;

        if pos >= data.len() {
            return Err("Not enough data for tag");
        }
        let tag = match data[pos] {
            1 => WordTag::Monolingual,
            2 => WordTag::English,
            3 => WordTag::Both,
            _ => return Err("Invalid tag value"),
        };
        pos += 1;

        if pos >= data.len() {
            return Err("Not enough data for entry count");
        }
        let entry_count = data[pos] as usize;
        pos += 1;

        let mut entries = Vec::with_capacity(entry_count);
        for _ in 0..entry_count {
            if pos + 2 > data.len() {
                return Err("Not enough data for entry size");
            }
            let entry_size = u16::from_le_bytes([data[pos], data[pos + 1]]) as usize;
            pos += 2;

            if pos + entry_size > data.len() {
                return Err("Not enough data for entry");
            }
            let entry_data = &data[pos..pos + entry_size];
            pos += entry_size;

            let entry: WordEntryComplete =
                serde_json::from_slice(entry_data).map_err(|_| "Failed to deserialize entry")?;
            entries.push(entry);
        }

        // Deserialize sounds (optional)
        let sounds = if pos + 2 <= data.len() {
            let sounds_size = u16::from_le_bytes([data[pos], data[pos + 1]]) as usize;
            pos += 2;

            if sounds_size == 0 {
                None
            } else {
                if pos + sounds_size > data.len() {
                    return Err("Not enough data for sounds");
                }
                let sounds_data = &data[pos..pos + sounds_size];
                pos += sounds_size;

                let sound_str = String::from_utf8(sounds_data.to_vec())
                    .map_err(|_| "Failed to deserialize sounds as UTF-8")?;
                Some(sound_str)
            }
        } else {
            None
        };

        // Deserialize hyphenations (optional)
        let hyphenations = if pos + 2 <= data.len() {
            let hyphenations_size = u16::from_le_bytes([data[pos], data[pos + 1]]) as usize;
            pos += 2;

            if hyphenations_size == 0 {
                None
            } else {
                if pos + hyphenations_size > data.len() {
                    return Err("Not enough data for hyphenations");
                }
                let hyphenations_data = &data[pos..pos + hyphenations_size];
                pos += hyphenations_size;

                let hyphenation: Hyphenation = serde_json::from_slice(hyphenations_data)
                    .map_err(|_| "Failed to deserialize hyphenations")?;
                Some(hyphenation)
            }
        } else {
            None
        };

        Ok(WordWithTaggedEntries {
            word,
            tag,
            entries,
            sounds,
            hyphenations,
        })
    }
}
