use serde::{Deserialize, Serialize};

pub mod kaikki;
pub mod reader;

#[cfg(target_os = "android")]
pub mod android;

pub const HEADER_SIZE: u8 = 16;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WordEntryComplete {
    pub senses: Vec<Sense>,
    pub hyphenations: Vec<Hyphenation>,
    pub sounds: Vec<Sound>,
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
    pub form_of: Vec<FormOf>,
    pub glosses: Vec<Gloss>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FormOf {
    pub word: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Sound {
    pub ipa: Option<String>,
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

        Ok(WordWithTaggedEntries { word, tag, entries })
    }
}
