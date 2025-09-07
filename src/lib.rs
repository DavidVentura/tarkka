pub mod kaikki;
pub mod reader;
pub mod ser;
//pub mod test_enum;
//pub mod test_ser;
use ser::CompactSerialize;

#[cfg(target_os = "android")]
pub mod android;

pub const HEADER_SIZE: u8 = 16;

#[derive(Debug, Clone, CompactSerialize)]
pub struct WordEntryComplete {
    #[max_len_cat(OneByte)]
    pub senses: Vec<Sense>,
}

#[derive(Debug, Clone, CompactSerialize)]
pub struct Gloss {
    pub shared_prefix_count: u8,
    #[max_len_cat(OneByte)]
    pub new_categories: Vec<String>,
    #[max_len_cat(TwoBytesVar)]
    pub gloss: String,
}

#[derive(Debug, Clone, CompactSerialize)]
pub struct Sense {
    #[max_len_cat(OneByte)]
    pub pos: String,
    #[max_len_cat(OneByte)]
    pub glosses: Vec<Gloss>,
}

#[derive(Debug, Clone, Copy, CompactSerialize)]
#[repr(u8)]
pub enum WordTag {
    Monolingual = 1,
    English = 2,
    Both = 3,
}

#[derive(Debug, CompactSerialize)]
pub struct WordWithTaggedEntries {
    pub tag: WordTag,
    #[max_len_cat(OneByte)]
    pub word: String,
    #[max_len_cat(OneByte)]
    pub entries: Vec<WordEntryComplete>,
    pub sounds: Option<String>,
    #[max_len_cat(OneByte)]
    pub hyphenations: Vec<String>,
}

impl WordWithTaggedEntries {
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

            todo!();
            //let entry: WordEntryComplete =
            //    serde_json::from_slice(entry_data).map_err(|_| "Failed to deserialize entry")?;
            //entries.push(entry);
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

                let hyphenation: kaikki::Hyphenation = serde_json::from_slice(hyphenations_data)
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
            hyphenations: if let Some(h) = hyphenations {
                h.parts
            } else {
                vec![]
            },
        })
    }
}
