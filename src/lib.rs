use serde::{Deserialize, Serialize};

pub mod reader;

#[cfg(target_os = "android")]
pub mod android;

pub const HEADER_SIZE: u8 = 16;

#[derive(Debug, Deserialize, Serialize)]
pub struct WordEntry {
    pub pos: Option<String>,
    pub lang_code: Option<String>,
    pub word: Option<String>,
}
#[derive(Debug, Deserialize, Serialize)]
pub struct WordEntryComplete {
    pub pos: String,
    pub word: String,
    pub senses: Vec<Sense>,
    pub hyphenations: Option<Vec<Hyphenation>>,
    pub sounds: Option<Vec<Sound>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Sense {
    pub form_of: Option<Vec<FormOf>>,
    pub glosses: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct FormOf {
    pub word: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Sound {
    pub ipa: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Hyphenation {
    pub parts: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AggregatedWord {
    #[serde(skip_serializing, skip_deserializing)]
    pub word: String,
    pub pos_glosses: Vec<PosGlosses>,
    pub hyphenation: Option<Vec<String>>,
    pub form_of: Option<Vec<String>>,
    pub ipa_sound: Option<Vec<String>>,
}

impl AggregatedWord {
    pub fn serialize(&self) -> Vec<u8> {
        let mut v = Vec::new();

        assert!(self.pos_glosses.len() < 256);
        v.extend_from_slice(&[self.pos_glosses.len() as u8]);
        for pos_gloss in &self.pos_glosses {
            v.extend_from_slice(&pos_gloss.serialize());
        }

        let hyphenation_len = self.hyphenation.as_ref().map(|h| h.len()).unwrap_or(0);
        assert!(hyphenation_len < 256);
        v.extend_from_slice(&[hyphenation_len as u8]);
        if let Some(hyphenation) = &self.hyphenation {
            for part in hyphenation {
                if part.len() >= 256 {
                    // FIXME: some word in the dict (mulinello) has garbage
                    println!("word '{}' hwas more than 256 syllab: {:?}", self.word, part);
                    v.extend_from_slice(&[0u8]);
                } else {
                    assert!(part.len() < 256, "w {} syllab {:?}", self.word, part);
                    v.extend_from_slice(&[part.len() as u8]);
                    v.extend_from_slice(part.as_bytes());
                }
            }
        }

        let form_of_len = self.form_of.as_ref().map(|f| f.len()).unwrap_or(0);
        assert!(form_of_len < 256);
        v.extend_from_slice(&[form_of_len as u8]);
        if let Some(form_of) = &self.form_of {
            for word in form_of {
                assert!(word.len() < 256);
                v.extend_from_slice(&[word.len() as u8]);
                v.extend_from_slice(word.as_bytes());
            }
        }

        let ipa_sound_len = self.ipa_sound.as_ref().map(|i| i.len()).unwrap_or(0);
        assert!(ipa_sound_len < 256);
        v.extend_from_slice(&[ipa_sound_len as u8]);
        if let Some(ipa_sound) = &self.ipa_sound {
            for sound in ipa_sound {
                assert!(sound.len() < 256, "word {:?} sound {:?}", self.word, sound);
                v.extend_from_slice(&[sound.len() as u8]);
                v.extend_from_slice(sound.as_bytes());
            }
        }

        v
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Gloss {
    pub shared_prefix_count: u8, // How many categories shared with previous gloss
    pub new_categories: Vec<String>, // Only the new/different categories
    pub gloss: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PosGlosses {
    pub pos: String,
    pub glosses: Vec<Gloss>,
}

impl PosGlosses {
    fn serialize(&self) -> Vec<u8> {
        let mut v = Vec::new();
        assert!(self.pos.len() < 256);
        v.extend_from_slice(&[self.pos.len() as u8]);
        v.extend_from_slice(self.pos.as_bytes());

        assert!(self.glosses.len() < 256, "wtf??? {:?}", self.glosses);
        v.extend_from_slice(&[self.glosses.len() as u8]);
        for gloss in &self.glosses {
            v.extend_from_slice(&gloss.serialize());
        }
        v
    }
}

impl Gloss {
    pub fn get_category_path(&self, previous_path: &[String]) -> Vec<String> {
        let mut path = Vec::new();
        if self.shared_prefix_count > 0 {
            let prefix_len = (self.shared_prefix_count as usize).min(previous_path.len());
            path.extend_from_slice(&previous_path[..prefix_len]);
        }
        path.extend(self.new_categories.clone());
        path
    }

    fn serialize(&self) -> Vec<u8> {
        let mut v = Vec::new();

        // Pack shared_prefix_count and new_categories.len() into single byte (4 bits each)
        assert!(
            self.shared_prefix_count < 16,
            "shared_prefix_count must be < 16, got {}",
            self.shared_prefix_count
        );
        assert!(
            self.new_categories.len() < 16,
            "new_categories.len() must be < 16, got {}",
            self.new_categories.len()
        );

        let packed_byte = (self.shared_prefix_count << 4) | (self.new_categories.len() as u8);
        v.extend_from_slice(&[packed_byte]);

        // Serialize new categories
        for category in &self.new_categories {
            v.extend_from_slice(&u16_as_2b_leb128(category.len() as u16));
            v.extend_from_slice(category.as_bytes());
        }

        // Serialize gloss
        v.extend_from_slice(&u16_as_2b_leb128(self.gloss.len() as u16));
        v.extend_from_slice(self.gloss.as_bytes());
        v
    }
}

/// reserves 1 bit to indicate if it takes 1 byte or 2
/// max value == 2**15-1 == 32767
pub fn u16_as_2b_leb128(value: u16) -> Vec<u8> {
    if value < 0x80 {
        vec![value as u8]
    } else if value < 32767 {
        vec![(value as u8) | 0x80, (value >> 7) as u8]
    } else {
        panic!("Tried to fit too large val into 2b");
    }
}

pub fn read_2b_leb128(data: &[u8], pos: &mut usize) -> Result<u16, &'static str> {
    if *pos >= data.len() {
        return Err("Not enough data");
    }

    let first_byte = data[*pos];
    *pos += 1;

    if first_byte < 0x80 {
        Ok(first_byte as u16)
    } else {
        if *pos >= data.len() {
            return Err("Not enough data for second byte");
        }
        let second_byte = data[*pos];
        *pos += 1;
        Ok(((first_byte & 0x7F) as u16) | ((second_byte as u16) << 7))
    }
}

impl AggregatedWord {
    pub fn deserialize(data: &[u8], word: String) -> Result<Self, &'static str> {
        let mut pos = 0;

        if pos >= data.len() {
            return Err("Not enough data for pos_glosses count");
        }
        let pos_glosses_count = data[pos] as usize;
        pos += 1;

        let mut pos_glosses = Vec::with_capacity(pos_glosses_count);
        for _ in 0..pos_glosses_count {
            if pos >= data.len() {
                return Err("Not enough data for pos length");
            }
            let pos_len = data[pos] as usize;
            pos += 1;

            if pos + pos_len > data.len() {
                return Err("Not enough data for pos string");
            }
            let pos_string = String::from_utf8_lossy(&data[pos..pos + pos_len]).to_string();
            pos += pos_len;

            if pos >= data.len() {
                return Err("Not enough data for glosses count");
            }
            let glosses_count = data[pos] as usize;
            pos += 1;

            let mut glosses = Vec::with_capacity(glosses_count);
            let mut last_category_path: Vec<String> = Vec::new();

            for _ in 0..glosses_count {
                if pos >= data.len() {
                    return Err("Not enough data for packed byte");
                }
                let packed_byte = data[pos];
                pos += 1;

                // Unpack shared_prefix_count (upper 4 bits) and new_categories_count (lower 4 bits)
                let shared_prefix_count = (packed_byte >> 4) as usize;
                let new_categories_count = (packed_byte & 0x0F) as usize;

                let mut new_categories = Vec::with_capacity(new_categories_count);
                for _ in 0..new_categories_count {
                    let category_len = read_2b_leb128(data, &mut pos)? as usize;
                    if pos + category_len > data.len() {
                        return Err("Not enough data for category string");
                    }
                    let category_string =
                        String::from_utf8_lossy(&data[pos..pos + category_len]).to_string();
                    pos += category_len;
                    new_categories.push(category_string);
                }

                let gloss_len = read_2b_leb128(data, &mut pos)? as usize;
                if pos + gloss_len > data.len() {
                    return Err("Not enough data for gloss text");
                }
                let gloss = String::from_utf8_lossy(&data[pos..pos + gloss_len]).to_string();
                pos += gloss_len;

                // Reconstruct full category path for compatibility
                let mut category_path = Vec::new();
                if shared_prefix_count > 0 {
                    if shared_prefix_count > last_category_path.len() {
                        return Err("Shared prefix count exceeds previous category path length");
                    }
                    category_path.extend_from_slice(&last_category_path[..shared_prefix_count]);
                }
                category_path.extend(new_categories.clone());

                // Update last category path for next iteration
                last_category_path = category_path.clone();

                glosses.push(Gloss {
                    shared_prefix_count: shared_prefix_count as u8,
                    new_categories,
                    gloss,
                });
            }

            pos_glosses.push(PosGlosses {
                pos: pos_string,
                glosses,
            });
        }

        // Read hyphenation
        if pos >= data.len() {
            return Err("Not enough data for hyphenation count");
        }
        let hyphen_count = data[pos] as usize;
        pos += 1;

        let hyphenation = if hyphen_count == 0 {
            None
        } else {
            let mut parts = Vec::with_capacity(hyphen_count);
            for _ in 0..hyphen_count {
                if pos >= data.len() {
                    return Err("Not enough data for hyphenation part length");
                }
                let part_len = data[pos] as usize;
                pos += 1;

                if part_len > 0 {
                    if pos + part_len > data.len() {
                        return Err("Not enough data for hyphenation part");
                    }
                    let part = String::from_utf8_lossy(&data[pos..pos + part_len]).to_string();
                    pos += part_len;
                    parts.push(part);
                }
            }
            if parts.is_empty() { None } else { Some(parts) }
        };

        // Read form_of
        if pos >= data.len() {
            return Err("Not enough data for form_of count");
        }
        let form_of_count = data[pos] as usize;
        pos += 1;

        let form_of = if form_of_count == 0 {
            None
        } else {
            let mut words = Vec::with_capacity(form_of_count);
            for _ in 0..form_of_count {
                if pos >= data.len() {
                    return Err("Not enough data for form_of word length");
                }
                let word_len = data[pos] as usize;
                pos += 1;

                if pos + word_len > data.len() {
                    return Err("Not enough data for form_of word");
                }
                let word = String::from_utf8_lossy(&data[pos..pos + word_len]).to_string();
                pos += word_len;
                words.push(word);
            }
            Some(words)
        };

        // Read IPA sounds
        if pos >= data.len() {
            return Err("Not enough data for ipa count");
        }
        let ipa_count = data[pos] as usize;
        pos += 1;

        let ipa_sound = if ipa_count == 0 {
            None
        } else {
            let mut sounds = Vec::with_capacity(ipa_count);
            for _ in 0..ipa_count {
                if pos >= data.len() {
                    return Err("Not enough data for ipa length");
                }
                let sound_len = data[pos] as usize;
                pos += 1;

                if pos + sound_len > data.len() {
                    return Err("Not enough data for ipa sound");
                }
                let sound = String::from_utf8_lossy(&data[pos..pos + sound_len]).to_string();
                pos += sound_len;
                sounds.push(sound);
            }
            Some(sounds)
        };

        Ok(AggregatedWord {
            word,
            pos_glosses,
            hyphenation,
            form_of,
            ipa_sound,
        })
    }
}

