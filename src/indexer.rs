use itertools::Itertools;
use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, Write};
use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tarkka::kaikki::{KaikkiWordEntry, WordEntry};
use tarkka::{HEADER_SIZE, TARKKA_FMT_VERSION, WordEntryComplete, WordTag, WordWithTaggedEntries};
use threadpool::ThreadPool;

// Supported languages from Language.kt
const SUPPORTED_LANGUAGES: &[&str] = &[
    "sq", "ar", "az", "bn", "bg", "ca", "zh", "hr", "cs", "da", "nl", "en", "et", "fi", "fr", "de",
    "el", "gu", "he", "hi", "hu", "id", "it", "ja", "kn", "ko", "lv", "lt", "ms", "ml", "fa", "pl",
    "pt", "ro", "ru", "sk", "sl", "es", "sv", "ta", "te", "tr", "uk",
];

pub mod reader;
pub mod ser;
use tarkka::ser::{CompactSerialize, VarUint};
use zeekstd::{EncodeOptions, FrameSizePolicy};

fn lang_words(word_lang: &str, gloss_lang: &str, fname: &str) -> Vec<WordWithTaggedEntries> {
    let f = File::open(fname).unwrap();
    let s = Instant::now();
    let good_words = filter(word_lang, f);
    println!("Filter took {:?}", s.elapsed());
    let s = Instant::now();
    println!("serialize took {:?}", s.elapsed());
    let tag = match (word_lang, gloss_lang) {
        (_, "en") => WordTag::English,
        (x, y) if x == y => WordTag::Monolingual,
        (_, _) => panic!("idk what to do {word_lang} {gloss_lang}"),
    };
    let filtered: Vec<WordWithTaggedEntries> = good_words
        .into_iter()
        .map(|w| w.to_word_entry_complete(tag))
        .collect();

    filtered
}

fn create_dictionary(lang: &str, timestamp_s: u64) -> Result<String, Box<dyn std::error::Error>> {
    println!("Processing: {}", lang);

    let monolingual_path = format!("out/monolingual/{}.jsonl", lang);
    let english_path = format!("out/english/{}.jsonl", lang);

    // Check what files are available
    let has_monolingual = Path::new(&monolingual_path).exists();
    let has_english = Path::new(&english_path).exists();

    if !has_monolingual && !has_english {
        return Err(format!("No files available for {}", lang).into());
    }

    let is_multi = has_monolingual && has_english;
    let output_filename = if is_multi {
        format!("out/dictionaries/{}-multi-dictionary.dict", lang)
    } else {
        format!("out/dictionaries/{}-english-dictionary.dict", lang)
    };

    // Check if output file already exists
    if Path::new(&output_filename).exists() {
        println!("Dictionary already exists, skipping: {}", output_filename);
        return Ok(output_filename);
    }

    // Load available data
    let (good_words1, good_words2) = if has_monolingual && has_english {
        // Both available - multi dictionary
        let mono = lang_words(lang, lang, &monolingual_path);
        let eng = lang_words(lang, "en", &english_path);
        println!(
            "entries {} (mono) {} {} (eng) {}",
            lang.to_uppercase(),
            mono.len(),
            lang.to_uppercase(),
            eng.len()
        );
        (mono, eng)
    } else if has_english {
        // Only English available - english dictionary
        let eng = lang_words(lang, "en", &english_path);
        println!("entries {} (eng) {}", lang.to_uppercase(), eng.len());
        (Vec::new(), eng)
    } else {
        // Only monolingual available - treat as multi but with empty English
        let mono = lang_words(lang, lang, &monolingual_path);
        println!("entries {} (mono) {}", lang.to_uppercase(), mono.len());
        (mono, Vec::new())
    };

    let s = Instant::now();
    let words = build_tagged_index(good_words1, good_words2);
    println!("Build index took {:?}", s.elapsed());

    let s = Instant::now();

    let file = File::create(&output_filename)?;
    write_tagged(file, words, timestamp_s);
    println!("Writing took {:?}", s.elapsed());
    println!("Created: {}\n", output_filename);

    Ok(output_filename)
}

fn main() {
    let pool = ThreadPool::new(8);
    let created_dictionaries = Arc::new(AtomicUsize::new(0));
    let skipped_languages = Arc::new(AtomicUsize::new(0));
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    for &lang in SUPPORTED_LANGUAGES {
        let created_ref = Arc::clone(&created_dictionaries);
        let skipped_ref = Arc::clone(&skipped_languages);

        pool.execute(move || match create_dictionary(lang, now) {
            Ok(_) => {
                created_ref.fetch_add(1, Ordering::Relaxed);
            }
            Err(e) => {
                println!("Skipping {}: {}", lang, e);
                skipped_ref.fetch_add(1, Ordering::Relaxed);
            }
        });
    }

    pool.join();

    let created = created_dictionaries.load(Ordering::Relaxed);
    let skipped = skipped_languages.load(Ordering::Relaxed);

    println!(
        "Completed: {} dictionaries created, {} languages skipped",
        created, skipped
    );
}

fn common_prefix_len(a: &str, b: &str) -> usize {
    a.bytes().zip(b.bytes()).take_while(|(x, y)| x == y).count()
}

pub fn write_tagged<W: Write>(
    mut w: W,
    sorted_words: Vec<WordWithTaggedEntries>,
    timestamp_s: u64,
) {
    let s = Instant::now();
    let mut groups: BTreeMap<[u8; 3], Vec<&WordWithTaggedEntries>> = BTreeMap::new();
    for word in &sorted_words {
        let b = word.word.as_bytes();
        let l1_group = match b.len() {
            0 => panic!("got empty word"),
            1 => [0, 0, b[0]],
            2 => [0, b[0], b[1]],
            _ => [b[0], b[1], b[2]],
        };
        groups.entry(l1_group).or_default().push(&word);
    }

    for group in groups.values_mut() {
        group.sort_by(|a, b| a.word.as_bytes().cmp(&b.word.as_bytes()));
    }
    println!("grouped {:?}", s.elapsed());

    let mut level1_data = Vec::with_capacity(64 * 4096);

    let mut output = Vec::with_capacity(32 * 1024 * 1024);
    let mut all_serialized = Vec::with_capacity(32 * 1024 * 1024);
    let opts = EncodeOptions::new()
        .checksum_flag(false)
        .compression_level(9)
        .frame_size_policy(FrameSizePolicy::Uncompressed(1024 * 1024));
    let mut encoder = zeekstd::Encoder::with_opts(&mut output, opts).unwrap();
    let mut level2_size: u32 = 0;

    let mut shared_prefixes = 0;
    let mut global_binary_offset = 0u32;

    let mut under_1b = 0;
    let mut under_2b = 0;
    let mut over_2b = 0;
    for (l1_group, words) in groups {
        let mut l2_raw_size = 0u32;
        let mut prev_word = "";
        let group_binary_start = global_binary_offset;

        for word in words {
            let current_word = &word.word;
            let shared_len = common_prefix_len(prev_word, current_word);
            let suffix = &current_word.as_bytes()[shared_len..];

            shared_prefixes += shared_len;
            assert!(shared_len <= 127, "Shared prefix too long: {}", shared_len);
            assert!(suffix.len() <= 255, "Suffix too long: {}", suffix.len());
            assert!(
                suffix.len() > 0,
                "No suffix = duplicated word? {}",
                word.word
            );

            let ser_size = word.serialize(&mut all_serialized).unwrap();
            assert!(
                ser_size <= (u16::MAX / 2u16) as usize,
                "word too long {:#?}",
                word
            );
            let ss: VarUint = ser_size.into();
            if ser_size < 127 {
                under_1b += 1;
            } else if ser_size < 32767 {
                under_2b += 1;
            } else {
                over_2b += 1;
            }

            encoder.write(&[shared_len as u8]).unwrap();
            encoder.write(&[suffix.len() as u8]).unwrap();
            encoder.write(suffix).unwrap();
            let vlen = ss.serialize(&mut encoder).unwrap();

            let fixed_ovh = 2 + vlen;
            let entry_size = suffix.len() + fixed_ovh;
            global_binary_offset += ser_size as u32;
            l2_raw_size += entry_size as u32;

            prev_word = current_word;
        }

        // L1 size ~ 71KB (stays in memory, need to read entirely)
        // L2 size 5~10MB (seek, useful to not store entire word ever)
        level1_data.extend(l1_group);
        level1_data.extend(l2_raw_size.to_le_bytes());
        level1_data.extend(group_binary_start.to_le_bytes());
        assert!(level1_data.len() % 11 == 0);
        // each entry == 11 bytes, assert this in a better way
        // l1_group == 3
        // l2_raw_size == 4 bytes
        // group_binary_start == 4 bytes

        level2_size += l2_raw_size as u32;
    }
    println!("ser size: under1 {under_1b} under2 {under_2b} over2 {over_2b}");
    println!("saved {shared_prefixes}b with prefix thing");
    println!("serialized size = {}b", all_serialized.len());
    println!("compressed {:?}", s.elapsed());

    let mut total_ser_size = 0u32;
    encoder.write_all(&all_serialized).unwrap();
    total_ser_size += all_serialized.len() as u32;
    encoder.finish().unwrap();
    println!("finish compress {:?}", s.elapsed());

    w.write_all(b"DICT").unwrap();
    w.write_all(&(level1_data.len() as u32).to_le_bytes())
        .unwrap();
    w.write_all(&(level2_size as u32).to_le_bytes()).unwrap();
    w.write_all(&total_ser_size.to_le_bytes()).unwrap();
    // ^16
    w.write_all(&timestamp_s.to_le_bytes()).unwrap();
    // ^24
    w.write_all(&[TARKKA_FMT_VERSION]).unwrap();
    // ^25
    // reserved 7 bytes
    w.write_all(&[0, 0, 0]).unwrap(); // 28
    w.write_all(&0u32.to_le_bytes()).unwrap(); // 32
    w.write_all(&level1_data).unwrap();
    w.write_all(&output).unwrap();
    w.flush().unwrap();

    // output = compressed, level2_size = raw
    let compressed_ser_sz = output.len() - level2_size as usize;

    println!("Created dictionary.dict with {} words", sorted_words.len());
    println!("Header size (static) {}", HEADER_SIZE);
    println!("Level 1 starts at {}", HEADER_SIZE);
    println!("Level 1 size: {} bytes", level1_data.len());
    println!(
        "Level 2 starts at {}",
        HEADER_SIZE as usize + level1_data.len()
    );
    println!("Level 2 size: {} bytes", level2_size);
    println!(
        "Level 2 ends at bytes {}",
        HEADER_SIZE as u32 + level1_data.len() as u32 + level2_size
    );
    println!(
        "data size: raw {} compressed {}",
        total_ser_size, compressed_ser_sz
    );
}

fn filter<R: Read + Seek>(wanted_lang: &str, raw_data: R) -> Vec<KaikkiWordEntry> {
    let reader = BufReader::new(raw_data);
    let lines = reader.lines();
    let unwanted_pos = vec!["proverb"];
    let mut words: Vec<KaikkiWordEntry> = Vec::with_capacity(1_000_000);

    for line in lines {
        let line = line.unwrap();
        if line.len() == 0 {
            continue;
        }

        // First parse as basic WordEntry to check lang and word validity
        let word_entry: WordEntry = serde_json::from_str(&line).unwrap();
        match word_entry.lang_code {
            None => continue,
            Some(lang_code) => {
                if lang_code != wanted_lang {
                    continue;
                }
            }
        }
        match word_entry.word {
            Some(w) => {
                // special chinese comma
                if w.contains(" ") || w.contains("，") {
                    // phrases, like 'animal doméstico' don't make sense
                    // in a WORD dictionary
                    continue;
                }
            }
            None => {
                // a non-word word?
                continue;
            }
        }

        // ^^ parseable
        // vv parse as complete entry
        let mut kaikki_word: KaikkiWordEntry = serde_json::from_str(&line).unwrap();

        // Check POS at the sense level
        if let Some(ref pos) = kaikki_word.pos {
            if unwanted_pos.contains(&pos.as_str()) {
                continue;
            }
        }

        // no definitions, not the most useful dictionary
        if kaikki_word.senses.is_empty() {
            continue;
        }

        kaikki_word.sounds.retain_mut(|s| s.ipa.is_some());

        words.push(kaikki_word);
    }
    words
}

fn aggregate_entries(entries: Vec<WordWithTaggedEntries>) -> WordEntryComplete {
    if entries.is_empty() {
        panic!("Cannot aggregate empty entries");
    }

    if entries.len() == 1 {
        let entry_data = entries.into_iter().next().unwrap();
        let mut word_entry = entry_data.entries.into_iter().next().unwrap_or_else(|| {
            panic!("WordWithTaggedEntries must have at least one entry");
        });
        // Still need to compress categories and merge senses even for single entry
        merge_same_pos_senses(&mut word_entry.senses);
        return word_entry;
    }

    // Take the first entry as base and aggregate others into it
    let mut base = entries[0].entries[0].clone();

    for entry_data in entries.into_iter() {
        for word_entry in entry_data.entries {
            // Aggregate senses with POS preserved in each sense
            for sense in word_entry.senses {
                base.senses.push(sense);
            }
        }
    }

    // Merge senses with the same POS and compress categories after aggregation
    merge_same_pos_senses(&mut base.senses);
    base
}

fn merge_same_pos_senses(senses: &mut Vec<tarkka::Sense>) {
    use std::collections::HashMap;

    let mut pos_to_sense: HashMap<String, tarkka::Sense> = HashMap::new();

    for sense in senses.drain(..) {
        match pos_to_sense.get_mut(&sense.pos) {
            Some(existing_sense) => {
                // Merge glosses
                existing_sense.glosses.extend(sense.glosses);
                existing_sense.glosses = existing_sense.glosses.iter().cloned().unique().collect();
            }
            None => {
                pos_to_sense.insert(sense.pos.clone(), sense);
            }
        }
    }

    // Convert back to Vec and sort by POS for consistent ordering
    let mut merged_senses: Vec<tarkka::Sense> = pos_to_sense.into_values().collect();
    merged_senses.sort_by(|a, b| a.pos.cmp(&b.pos));
    *senses = merged_senses;
}

pub fn build_tagged_index(
    monolingual_entries: Vec<WordWithTaggedEntries>,
    english_entries: Vec<WordWithTaggedEntries>,
) -> Vec<WordWithTaggedEntries> {
    let mut word_groups: HashMap<String, (Vec<WordWithTaggedEntries>, Vec<WordWithTaggedEntries>)> =
        HashMap::new();

    // Populate monolingual entries
    for entry in monolingual_entries {
        let word_str = entry.word.clone();
        word_groups
            .entry(word_str)
            .or_insert((Vec::new(), Vec::new()))
            .0
            .push(entry);
    }

    // Populate English entries
    for entry in english_entries {
        let word_str = entry.word.clone();
        word_groups
            .entry(word_str)
            .or_insert((Vec::new(), Vec::new()))
            .1
            .push(entry);
    }

    // Helper to extract sounds and hyphenations from entries
    let extract_sound_and_hyph =
        |entries: &[WordWithTaggedEntries]| -> (Option<String>, Vec<String>) {
            let selected_sound = entries.iter().find_map(|e| e.sounds.clone());
            let selected_hyphenation = entries
                .iter()
                .find(|e| !e.hyphenations.is_empty())
                .map(|e| e.hyphenations.clone())
                .unwrap_or_default();
            (selected_sound, selected_hyphenation)
        };

    // Get all words in alphabetical order
    let mut words: Vec<String> = word_groups.keys().cloned().collect();
    words.sort();

    let mut result: Vec<WordWithTaggedEntries> = Vec::new();

    for word in words {
        let (mono_entries, eng_entries) = word_groups.remove(&word).unwrap();

        let tag = match (mono_entries.is_empty(), eng_entries.is_empty()) {
            (false, true) => WordTag::Monolingual,
            (true, false) => WordTag::English,
            (false, false) => WordTag::Both,
            (true, true) => unreachable!("Empty word group"),
        };

        let (entries, selected_sound, selected_hyphenation) = match tag {
            WordTag::Monolingual => {
                let (sound, hyph) = extract_sound_and_hyph(&mono_entries);
                let entry = aggregate_entries(mono_entries);
                (vec![entry], sound, hyph)
            }
            WordTag::English => {
                let (sound, hyph) = extract_sound_and_hyph(&eng_entries);
                let entry = aggregate_entries(eng_entries);
                (vec![entry], sound, hyph)
            }
            WordTag::Both => {
                // Prefer monolingual sound/hyphenation, fallback to English
                let (mono_sound, mono_hyph) = extract_sound_and_hyph(&mono_entries);
                let (eng_sound, eng_hyph) = extract_sound_and_hyph(&eng_entries);

                let eng_entry = aggregate_entries(eng_entries);
                let mono_entry = aggregate_entries(mono_entries);

                let selected_sound = mono_sound.or(eng_sound);
                let selected_hyphenation = if !mono_hyph.is_empty() {
                    mono_hyph
                } else {
                    eng_hyph
                };

                (
                    vec![mono_entry, eng_entry],
                    selected_sound,
                    selected_hyphenation,
                )
            }
        };

        result.push(WordWithTaggedEntries {
            word,
            tag,
            entries,
            sounds: selected_sound,
            hyphenations: selected_hyphenation,
        });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::DictionaryReader;
    use std::{
        io::Cursor,
        time::{SystemTime, UNIX_EPOCH},
    };
    use tarkka::{WordEntryComplete, WordTag};

    fn create_test_word(
        _word: &str,
        pos: &str,
        gloss: &str,
    ) -> (
        WordEntryComplete,
        Vec<tarkka::kaikki::Sound>,
        Vec<tarkka::kaikki::Hyphenation>,
    ) {
        (
            WordEntryComplete {
                senses: vec![tarkka::Sense {
                    pos: pos.to_string(),
                    glosses: vec![tarkka::Gloss {
                        gloss_lines: vec![gloss.to_string()],
                    }],
                }],
            },
            vec![],
            vec![],
        )
    }

    #[test]
    fn test_build_tagged_index() {
        let test_words = vec![
            {
                let (entry, sounds, hyphenations) =
                    create_test_word("dictate", "verb", "to say words aloud");
                ("dictate".to_string(), entry, sounds, hyphenations, true)
            },
            {
                let (entry, sounds, hyphenations) =
                    create_test_word("dictionary", "noun", "a book of word definitions");
                ("dictionary".to_string(), entry, sounds, hyphenations, true)
            },
            {
                let (entry, sounds, hyphenations) =
                    create_test_word("dictionary", "noun", "a reference book");
                ("dictionary".to_string(), entry, sounds, hyphenations, false)
            },
            {
                let (entry, sounds, hyphenations) =
                    create_test_word("dictoto", "noun", "fictional word for testing");
                ("dictoto".to_string(), entry, sounds, hyphenations, true)
            },
            {
                let (entry, sounds, hyphenations) = create_test_word("pa", "noun", "short word");
                ("pa".to_string(), entry, sounds, hyphenations, false)
            },
            {
                let (entry, sounds, hyphenations) = create_test_word("papa", "noun", "father");
                ("papa".to_string(), entry, sounds, hyphenations, true)
            },
            {
                let (entry, sounds, hyphenations) = create_test_word("papo", "noun", "chat");
                ("papo".to_string(), entry, sounds, hyphenations, true)
            },
            {
                let (entry, sounds, hyphenations) =
                    create_test_word("potato", "noun", "a vegetable");
                ("potato".to_string(), entry, sounds, hyphenations, false)
            },
        ];

        let result = build_tagged_index(test_words);

        assert_eq!(result.len(), 7);

        let words: Vec<&str> = result.iter().map(|w| w.word.as_str()).collect();
        assert_eq!(
            words,
            vec![
                "dictate",
                "dictionary",
                "dictoto",
                "pa",
                "papa",
                "papo",
                "potato"
            ]
        );

        // Check tags
        let dictionary = result.iter().find(|w| w.word == "dictionary").unwrap();
        assert!(matches!(dictionary.tag, WordTag::Both));
        assert_eq!(dictionary.entries.len(), 2); // Exactly 2 entries for Both tag

        let dictate = result.iter().find(|w| w.word == "dictate").unwrap();
        assert!(matches!(dictate.tag, WordTag::Monolingual));
        assert_eq!(dictate.entries.len(), 1);

        let pa = result.iter().find(|w| w.word == "pa").unwrap();
        assert!(matches!(pa.tag, WordTag::English));
        assert_eq!(pa.entries.len(), 1);
    }

    #[test]
    fn test_tagged_write_read_roundtrip() {
        let test_words = vec![
            {
                let (entry, sounds, hyphenations) =
                    create_test_word("dictate", "verb", "to say words aloud");
                ("dictate".to_string(), entry, sounds, hyphenations, true)
            },
            {
                let (entry, sounds, hyphenations) =
                    create_test_word("dictionary", "noun", "a book of word definitions");
                ("dictionary".to_string(), entry, sounds, hyphenations, true)
            },
            {
                let (entry, sounds, hyphenations) =
                    create_test_word("dictionary", "noun", "reference book");
                ("dictionary".to_string(), entry, sounds, hyphenations, false)
            },
            {
                let (entry, sounds, hyphenations) =
                    create_test_word("dictoto", "noun", "fictional word for testing");
                ("dictoto".to_string(), entry, sounds, hyphenations, true)
            },
            {
                let (entry, sounds, hyphenations) = create_test_word("pa", "noun", "short word");
                ("pa".to_string(), entry, sounds, hyphenations, false)
            },
            {
                let (entry, sounds, hyphenations) = create_test_word("papa", "noun", "father");
                ("papa".to_string(), entry, sounds, hyphenations, true)
            },
            {
                let (entry, sounds, hyphenations) = create_test_word("papo", "noun", "chat");
                ("papo".to_string(), entry, sounds, hyphenations, true)
            },
            {
                let (entry, sounds, hyphenations) =
                    create_test_word("potato", "noun", "a vegetable");
                ("potato".to_string(), entry, sounds, hyphenations, false)
            },
        ];

        let tagged_words = build_tagged_index(test_words);

        let mut buffer = Vec::new();
        let now = SystemTime::now().duration_since(UNIX_EPOCH);
        write_tagged(&mut buffer, tagged_words, now);

        let cursor = Cursor::new(buffer);
        let mut dict_reader = DictionaryReader::open(cursor).unwrap();

        let result = dict_reader.lookup("dictionary").unwrap();
        assert!(result.is_some());
        let word = result.unwrap();
        assert_eq!(word.word, "dictionary");
        assert!(matches!(word.tag, WordTag::Both));
        assert_eq!(word.entries.len(), 2); // Exactly 2 entries for Both tag
        assert_eq!(word.entries[0].senses[0].pos, "noun"); // First entry is monolingual
        assert_eq!(
            word.entries[0].senses[0].glosses[0].gloss_lines[0],
            "a book of word definitions"
        );
        assert_eq!(word.entries[1].senses[0].pos, "noun"); // Second entry is English
        assert_eq!(
            word.entries[1].senses[0].glosses[0].gloss_lines[0],
            "reference book"
        );

        let result = dict_reader.lookup("papa").unwrap();
        assert!(result.is_some());
        let word = result.unwrap();
        assert_eq!(word.word, "papa");
        assert!(matches!(word.tag, WordTag::Monolingual));
        assert_eq!(word.entries.len(), 1);
        assert_eq!(word.entries[0].senses[0].pos, "noun");

        let result = dict_reader.lookup("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_merge_same_pos_senses() {
        let mut entry = WordEntryComplete {
            senses: vec![
                tarkka::Sense {
                    pos: "noun".to_string(),
                    glosses: vec![tarkka::Gloss {
                        gloss_lines: vec!["first noun definition".to_string()],
                    }],
                },
                tarkka::Sense {
                    pos: "adj".to_string(),
                    glosses: vec![tarkka::Gloss {
                        gloss_lines: vec!["adjective definition".to_string()],
                    }],
                },
                tarkka::Sense {
                    pos: "noun".to_string(),
                    glosses: vec![tarkka::Gloss {
                        gloss_lines: vec!["second noun definition".to_string()],
                    }],
                },
            ],
        };

        merge_same_pos_senses(&mut entry.senses);

        // Should now have only 2 senses: 1 adj and 1 noun (with 2 glosses)
        assert_eq!(entry.senses.len(), 2);

        // Find the noun sense (should be first due to sorting)
        let noun_sense = entry.senses.iter().find(|s| s.pos == "noun").unwrap();
        assert_eq!(noun_sense.glosses.len(), 2);

        let adj_sense = entry.senses.iter().find(|s| s.pos == "adj").unwrap();
        assert_eq!(adj_sense.glosses.len(), 1);
    }
}
