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

#[derive(Debug, Deserialize, Serialize)]
pub struct PosGlosses {
    pub pos: String,
    pub glosses: Vec<String>,
}
