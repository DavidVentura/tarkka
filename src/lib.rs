use std::io::Read;
pub mod de;
pub mod kaikki;
pub mod reader;
pub mod ser;
pub mod test_skip;
pub mod test_varuint;
//pub mod test_enum;
//pub mod test_ser;
use de::CompactDeserialize;
use ser::CompactSerialize;

use crate::de::DeserializeError;

#[cfg(target_os = "android")]
pub mod android;

pub const HEADER_SIZE: u8 = 16;

#[derive(Debug, Clone, CompactDeserialize, CompactSerialize)]
pub struct WordEntryComplete {
    #[max_len_cat(OneByte)]
    pub senses: Vec<Sense>,
}

#[derive(Debug, Clone, CompactDeserialize, CompactSerialize)]
pub struct Gloss {
    #[skip]
    pub shared_prefix_count: u8,
    //#[max_len_cat(OneByte)]
    #[skip]
    pub new_categories: Vec<String>,
    #[max_len_cat(TwoBytesVar)]
    pub gloss: String,
}

#[derive(Debug, Clone, CompactDeserialize, CompactSerialize)]
pub struct Sense {
    #[max_len_cat(OneByte)]
    pub pos: String,
    #[max_len_cat(OneByte)]
    pub glosses: Vec<Gloss>,
}

#[derive(Debug, Clone, Copy, CompactDeserialize, CompactSerialize)]
#[repr(u8)]
pub enum WordTag {
    Monolingual = 1,
    English = 2,
    Both = 3,
}

#[derive(Debug, CompactSerialize, CompactDeserialize)]
pub struct WordWithTaggedEntries {
    pub tag: WordTag,
    #[skip]
    pub word: String,
    #[max_len_cat(OneByte)]
    pub entries: Vec<WordEntryComplete>,
    pub sounds: Option<String>,
    #[max_len_cat(OneByte)]
    pub hyphenations: Vec<String>,
}

impl WordWithTaggedEntries {
    pub fn named_deserialize<R: Read>(
        data: &mut R,
        word: String,
    ) -> Result<Self, DeserializeError> {
        let mut w = Self::deserialize(data).expect("failed to deserialze");
        w.word = word;
        return Ok(w);
    }
}
