use serde::{Deserialize, Serialize};

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

#[derive(Debug, Deserialize, Serialize)]
pub struct PosGlosses {
    pub pos: String,
    pub glosses: Vec<String>,
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
            v.extend_from_slice(&u16_as_2b_leb128(gloss.len() as u16));
            v.extend_from_slice(gloss.as_bytes());
        }
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
