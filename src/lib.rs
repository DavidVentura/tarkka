use std::io::Read;
pub mod de;
pub mod kaikki;
pub mod reader;
pub mod ser;
use de::CompactDeserialize;
use ser::CompactSerialize;

use crate::de::DeserializeError;

pub const HEADER_SIZE: u8 = 32;
pub const TARKKA_FMT_VERSION: u8 = 1;

#[derive(Debug, Clone, CompactDeserialize, CompactSerialize)]
pub struct WordEntryComplete {
    #[max_len_cat(OneByte)]
    pub senses: Vec<Sense>,
}

#[derive(Debug, Clone, CompactDeserialize, CompactSerialize, Hash, PartialEq, Eq)]
pub struct Gloss {
    #[max_len_cat(OneByte)]
    pub gloss_lines: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, CompactDeserialize, CompactSerialize)]
#[repr(u8)]
pub enum PartOfSpeech {
    Affix = 1,
    CombiningForm = 2,
    Proverb = 3,
    Postp = 4,
    Article = 5,
    Interfix = 6,
    Infix = 7,
    Punct = 8,
    Particle = 9,
    PrepPhrase = 10,
    Character = 11,
    Det = 12,
    Conj = 13,
    Num = 14,
    Symbol = 15,
    Prep = 16,
    Pron = 17,
    Contraction = 18,
    Phrase = 19,
    Suffix = 20,
    Prefix = 21,
    Intj = 22,
    Adv = 23,
    Name = 24,
    Verb = 25,
    Adj = 26,
    Noun = 27,
    Classifier = 28,
    Unknown = 29,
    AdjNoun = 30,
    Root = 31,
    Abbrev = 32,
    Counter = 33,
    Onomatopoeia = 34,
    Romanization = 35,
    SoftRedirect = 36,
    Circumfix = 37,
    TypographicVariant = 38,
    Participle = 39,
    Circumpos = 40,
    AdvPhrase = 41,
    Stem = 42,
    AdjPhrase = 43,
    Adnominal = 44,
    Syllable = 45,
    Gerund = 46,
}

impl TryFrom<&str> for PartOfSpeech {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "affix" => Ok(PartOfSpeech::Affix),
            "combining_form" => Ok(PartOfSpeech::CombiningForm),
            "proverb" => Ok(PartOfSpeech::Proverb),
            "postp" => Ok(PartOfSpeech::Postp),
            "article" => Ok(PartOfSpeech::Article),
            "interfix" => Ok(PartOfSpeech::Interfix),
            "infix" => Ok(PartOfSpeech::Infix),
            "punct" => Ok(PartOfSpeech::Punct),
            "particle" => Ok(PartOfSpeech::Particle),
            "prep_phrase" => Ok(PartOfSpeech::PrepPhrase),
            "character" => Ok(PartOfSpeech::Character),
            "det" => Ok(PartOfSpeech::Det),
            "conj" => Ok(PartOfSpeech::Conj),
            "num" => Ok(PartOfSpeech::Num),
            "symbol" => Ok(PartOfSpeech::Symbol),
            "prep" => Ok(PartOfSpeech::Prep),
            "pron" => Ok(PartOfSpeech::Pron),
            "contraction" => Ok(PartOfSpeech::Contraction),
            "phrase" => Ok(PartOfSpeech::Phrase),
            "suffix" => Ok(PartOfSpeech::Suffix),
            "prefix" => Ok(PartOfSpeech::Prefix),
            "intj" => Ok(PartOfSpeech::Intj),
            "interj" => Ok(PartOfSpeech::Intj),
            "adv" => Ok(PartOfSpeech::Adv),
            "name" => Ok(PartOfSpeech::Name),
            "verb" => Ok(PartOfSpeech::Verb),
            "adj" => Ok(PartOfSpeech::Adj),
            "noun" => Ok(PartOfSpeech::Noun),
            "classifier" => Ok(PartOfSpeech::Classifier),
            "unknown" => Ok(PartOfSpeech::Unknown),
            "adj_noun" => Ok(PartOfSpeech::AdjNoun),
            "root" => Ok(PartOfSpeech::Root),
            "abbrev" => Ok(PartOfSpeech::Abbrev),
            "counter" => Ok(PartOfSpeech::Counter),
            "onomatopoeia" => Ok(PartOfSpeech::Onomatopoeia),
            "onomatopeia" => Ok(PartOfSpeech::Onomatopoeia),
            "romanization" => Ok(PartOfSpeech::Romanization),
            "soft-redirect" => Ok(PartOfSpeech::SoftRedirect),
            "circumfix" => Ok(PartOfSpeech::Circumfix),
            "typographic variant" => Ok(PartOfSpeech::TypographicVariant),
            "participle" => Ok(PartOfSpeech::Participle),
            "circumpos" => Ok(PartOfSpeech::Circumpos),
            "adv_phrase" => Ok(PartOfSpeech::AdvPhrase),
            "stem" => Ok(PartOfSpeech::Stem),
            "adj_phrase" => Ok(PartOfSpeech::AdjPhrase),
            "adnominal" => Ok(PartOfSpeech::Adnominal),
            "syllable" => Ok(PartOfSpeech::Syllable),
            "gerund" => Ok(PartOfSpeech::Gerund),
            _ => Err(format!("Unknown part of speech: {}", value)),
        }
    }
}

impl TryFrom<String> for PartOfSpeech {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_from(value.as_str())
    }
}

impl std::fmt::Display for PartOfSpeech {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            PartOfSpeech::Affix => "affix",
            PartOfSpeech::CombiningForm => "combining_form",
            PartOfSpeech::Proverb => "proverb",
            PartOfSpeech::Postp => "postp",
            PartOfSpeech::Article => "article",
            PartOfSpeech::Interfix => "interfix",
            PartOfSpeech::Infix => "infix",
            PartOfSpeech::Punct => "punct",
            PartOfSpeech::Particle => "particle",
            PartOfSpeech::PrepPhrase => "prep_phrase",
            PartOfSpeech::Character => "character",
            PartOfSpeech::Det => "det",
            PartOfSpeech::Conj => "conj",
            PartOfSpeech::Num => "num",
            PartOfSpeech::Symbol => "symbol",
            PartOfSpeech::Prep => "prep",
            PartOfSpeech::Pron => "pron",
            PartOfSpeech::Contraction => "contraction",
            PartOfSpeech::Phrase => "phrase",
            PartOfSpeech::Suffix => "suffix",
            PartOfSpeech::Prefix => "prefix",
            PartOfSpeech::Intj => "intj",
            PartOfSpeech::Adv => "adv",
            PartOfSpeech::Name => "name",
            PartOfSpeech::Verb => "verb",
            PartOfSpeech::Adj => "adj",
            PartOfSpeech::Noun => "noun",
            PartOfSpeech::Classifier => "classifier",
            PartOfSpeech::Unknown => "unknown",
            PartOfSpeech::AdjNoun => "adj_noun",
            PartOfSpeech::Root => "root",
            PartOfSpeech::Abbrev => "abbrev",
            PartOfSpeech::Counter => "counter",
            PartOfSpeech::Onomatopoeia => "onomatopoeia",
            PartOfSpeech::Romanization => "romanization",
            PartOfSpeech::SoftRedirect => "soft-redirect",
            PartOfSpeech::Circumfix => "circumfix",
            PartOfSpeech::TypographicVariant => "typographic variant",
            PartOfSpeech::Participle => "participle",
            PartOfSpeech::Circumpos => "circumpos",
            PartOfSpeech::AdvPhrase => "adv_phrase",
            PartOfSpeech::Stem => "stem",
            PartOfSpeech::AdjPhrase => "adj_phrase",
            PartOfSpeech::Adnominal => "adnominal",
            PartOfSpeech::Syllable => "syllable",
            PartOfSpeech::Gerund => "gerund",
        };
        write!(f, "{}", s)
    }
}

#[derive(Debug, Clone, CompactDeserialize, CompactSerialize)]
pub struct Sense {
    pub pos: PartOfSpeech,
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
    #[max_len_cat(OneByte)]
    pub redirects: Vec<String>,
}

impl WordWithTaggedEntries {
    pub fn named_deserialize<R: Read>(
        data: &mut R,
        word: String,
    ) -> Result<Self, DeserializeError> {
        let mut w = Self::deserialize(data).expect("failed to deserialze");
        w.word = word;
        Ok(w)
    }
}
