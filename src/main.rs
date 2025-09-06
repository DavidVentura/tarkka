use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, Write};
use std::time::Instant;
use tarkka::kaikki::{KaikkiWordEntry, WordEntry};
use tarkka::{HEADER_SIZE, WordEntryComplete, WordTag, WordWithTaggedEntries};

pub mod reader;

fn lang_words(
    word_lang: &str,
    gloss_lang: &str,
) -> Vec<(
    String,
    WordEntryComplete,
    Vec<tarkka::kaikki::Sound>,
    Vec<tarkka::Hyphenation>,
    bool,
)> {
    let good_words = match File::open(format!(
        "filtered-{word_lang}-{gloss_lang}-raw-wiktextract-data.jsonl"
    )) {
        Ok(mut f) => {
            println!("parsing json from pre-filtered");
            let mut s = String::new();
            f.read_to_string(&mut s).unwrap();
            let kaikki_words: Vec<(String, KaikkiWordEntry)> =
                serde_json::from_str(s.as_str()).unwrap();
            let words: Vec<(
                String,
                WordEntryComplete,
                Vec<tarkka::kaikki::Sound>,
                Vec<tarkka::Hyphenation>,
            )> = kaikki_words
                .into_iter()
                .map(|(s, kw)| {
                    let (entry, sounds, hyphenations) = kw.to_word_entry_complete();
                    (s.clone(), entry, sounds, hyphenations)
                })
                .collect();
            let is_monolingual = word_lang == gloss_lang;
            words
                .into_iter()
                .map(|(word, w, sounds, hyphenations)| {
                    (word, w, sounds, hyphenations, is_monolingual)
                })
                .collect()
            // serde_json::from_reader(f).unwrap()
            // much slower??
        }
        Err(_) => {
            println!("filtered not found, creating");
            let f = File::open(format!("{gloss_lang}-raw-wiktextract-data.jsonl")).unwrap();
            let s = Instant::now();
            let good_words = filter(word_lang, f);
            println!("Filter took {:?}", s.elapsed());
            let s = Instant::now();
            {
                let mut f = File::create(format!(
                    "filtered-{word_lang}-{gloss_lang}-raw-wiktextract-data.jsonl"
                ))
                .unwrap();
                let ser = serde_json::to_string_pretty(&good_words).unwrap();
                f.write_all(ser.as_bytes()).unwrap();
            }
            println!("serialize took {:?}", s.elapsed());
            let is_monolingual = word_lang == gloss_lang;
            good_words
                .into_iter()
                .map(|(word, w)| {
                    let (entry, sounds, hyphenations) = w.to_word_entry_complete();
                    (word, entry, sounds, hyphenations, is_monolingual)
                })
                .collect()
        }
    };
    good_words
}

fn main() {
    /*
    let word_lang = "es";
    let good_words1 = lang_words(word_lang, "es");
    let mut good_words2 = lang_words(word_lang, "en");
    println!("entries ES {} EN {}", good_words1.len(), good_words2.len());
    let mut all_tagged_entries = good_words1;
    all_tagged_entries.append(&mut good_words2);

    */
    /*
    let word_lang = "en";
    let all_tagged_entries = lang_words(word_lang, "en");
    */
    let word_lang = "es";
    let all_tagged_entries = lang_words(word_lang, "es");

    let s = Instant::now();
    let words = build_tagged_index(all_tagged_entries);
    println!("Build index took {:?}", s.elapsed());

    let s = Instant::now();
    let file = File::create(format!("{word_lang}-multi-dictionary.dict")).unwrap();
    write_tagged(file, words);
    println!("writing took {:?}", s.elapsed());
}

fn common_prefix_len(a: &str, b: &str) -> usize {
    a.bytes().zip(b.bytes()).take_while(|(x, y)| x == y).count()
}

pub fn write_tagged<W: Write>(mut w: W, sorted_words: Vec<WordWithTaggedEntries>) {
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
    let mut encoder = zeekstd::Encoder::new(&mut output).unwrap();
    let mut level2_size: u32 = 0;

    let mut shared_prefixes = 0;
    let mut global_binary_offset = 0u32;

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

            let serialized = word.serialize();
            let ser_size = serialized.len();
            assert!(ser_size <= u16::MAX as usize);
            let ser_size_b = (ser_size as u16).to_le_bytes();

            encoder.write(&[shared_len as u8]).unwrap();
            encoder.write(&[suffix.len() as u8]).unwrap();
            encoder.write(suffix).unwrap();
            encoder.write(&ser_size_b).unwrap();

            let fixed_ovh = 1 + 1 + ser_size_b.len();
            debug_assert!(fixed_ovh <= 4);
            let entry_size = suffix.len() + fixed_ovh;
            all_serialized.extend_from_slice(&serialized);
            global_binary_offset += ser_size as u32;
            l2_raw_size += entry_size as u32;

            prev_word = current_word;
        }

        level1_data.extend(l1_group);
        level1_data.extend(l2_raw_size.to_le_bytes());
        level1_data.extend(group_binary_start.to_le_bytes());

        level2_size += l2_raw_size as u32;
    }
    println!("saved {shared_prefixes}b with prefix thing");
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
    w.write_all(&level1_data).unwrap();
    w.write_all(&output).unwrap();

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

fn filter<R: Read + Seek>(wanted_lang: &str, raw_data: R) -> Vec<(String, KaikkiWordEntry)> {
    let reader = BufReader::new(raw_data);
    let lines = reader.lines();
    let unwanted_pos = vec!["proverb"];
    let mut words: Vec<(String, KaikkiWordEntry)> = Vec::with_capacity(1_000_000);

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
                if w.contains("，") {
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

        let word_str = kaikki_word.word.clone();
        words.push((word_str, kaikki_word));
    }
    words
}

// TODO: this should not take shitty entries with the Kaikki limitatios
fn aggregate_entries(
    entries: Vec<(
        WordEntryComplete,
        Vec<tarkka::kaikki::Sound>,
        Vec<tarkka::Hyphenation>,
    )>,
    _is_monolingual_first: bool,
) -> (
    WordEntryComplete,
    Option<String>,
    Option<tarkka::Hyphenation>,
) {
    if entries.is_empty() {
        panic!("Cannot aggregate empty entries");
    }

    // Extract sounds and hyphenations from all entries
    let mut selected_sound = None;
    let mut selected_hyphenation = None;

    // Priority: first available (monolingual first if is_monolingual_first is true)
    for (_, sounds, hyphenations) in &entries {
        if selected_sound.is_none() && !sounds.is_empty() {
            if let Some(sound) = sounds.first() {
                if let Some(ref ipa) = sound.ipa {
                    selected_sound = Some(ipa.clone());
                }
            }
        }
        if selected_hyphenation.is_none() && !hyphenations.is_empty() {
            selected_hyphenation = hyphenations.first().cloned();
        }
        if selected_sound.is_some() && selected_hyphenation.is_some() {
            break;
        }
    }

    if entries.len() == 1 {
        let (mut entry, _, _) = entries.into_iter().next().unwrap();
        // Still need to compress categories and merge senses even for single entry
        merge_same_pos_senses(&mut entry.senses);
        compress_categories(&mut entry.senses);
        return (entry, selected_sound, selected_hyphenation);
    }

    // Take the first entry as base and aggregate others into it
    let mut base = entries[0].0.clone();

    for (entry, _, _) in entries.into_iter().skip(1) {
        // Aggregate senses with POS preserved in each sense
        for sense in entry.senses {
            base.senses.push(sense);
        }
    }

    // Merge senses with the same POS and compress categories after aggregation
    merge_same_pos_senses(&mut base.senses);
    compress_categories(&mut base.senses);

    (base, selected_sound, selected_hyphenation)
}

fn merge_same_pos_senses(senses: &mut Vec<tarkka::Sense>) {
    use std::collections::HashMap;

    let mut pos_to_sense: HashMap<String, tarkka::Sense> = HashMap::new();

    for sense in senses.drain(..) {
        match pos_to_sense.get_mut(&sense.pos) {
            Some(existing_sense) => {
                // Merge glosses
                existing_sense.glosses.extend(sense.glosses);
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

fn compress_categories(senses: &mut Vec<tarkka::Sense>) {
    for sense in senses {
        let mut last_category_path: Vec<String> = Vec::new();

        for gloss in sense.glosses.iter_mut() {
            if !gloss.new_categories.is_empty() {
                // Current full category path is the new categories for this gloss
                let current_category_path = gloss.new_categories.clone();

                // Find common prefix with previous category path within this sense
                let mut shared_count = 0;
                while shared_count < last_category_path.len()
                    && shared_count < current_category_path.len()
                    && last_category_path[shared_count] == current_category_path[shared_count]
                {
                    shared_count += 1;
                }

                // Update the gloss with compression info
                gloss.shared_prefix_count = shared_count as u8;
                gloss.new_categories = if shared_count < current_category_path.len() {
                    current_category_path[shared_count..].to_vec()
                } else {
                    vec![]
                };

                // Update last category path for next iteration within this sense
                last_category_path = current_category_path;
            } else {
                // No categories, reset the path
                gloss.shared_prefix_count = 0;
                last_category_path.clear();
            }
        }
    }
}

pub fn build_tagged_index(
    tagged_words: Vec<(
        String,
        WordEntryComplete,
        Vec<tarkka::kaikki::Sound>,
        Vec<tarkka::Hyphenation>,
        bool,
    )>,
) -> Vec<WordWithTaggedEntries> {
    let mut word_groups: HashMap<
        String,
        (
            Vec<(
                WordEntryComplete,
                Vec<tarkka::kaikki::Sound>,
                Vec<tarkka::Hyphenation>,
            )>,
            Vec<(
                WordEntryComplete,
                Vec<tarkka::kaikki::Sound>,
                Vec<tarkka::Hyphenation>,
            )>,
        ),
    > = HashMap::new();

    for (word_str, word_entry, sounds, hyphenations, is_monolingual) in tagged_words {
        let entry = word_groups
            .entry(word_str)
            .or_insert((Vec::new(), Vec::new()));
        if is_monolingual {
            entry.0.push((word_entry, sounds, hyphenations));
        } else {
            entry.1.push((word_entry, sounds, hyphenations));
        }
    }

    let mut result: Vec<WordWithTaggedEntries> = word_groups
        .into_iter()
        .map(|(word, (mono_entries, eng_entries))| {
            let tag = match (mono_entries.is_empty(), eng_entries.is_empty()) {
                (false, true) => WordTag::Monolingual,
                (true, false) => WordTag::English,
                (false, false) => WordTag::Both,
                (true, true) => unreachable!("Empty word group"),
            };

            // Aggregate entries of the same type into single comprehensive entries
            let (entries, selected_sound, selected_hyphenation) = match tag {
                WordTag::Monolingual => {
                    let (entry, sound, hyph) = aggregate_entries(mono_entries, true);
                    (vec![entry], sound, hyph)
                }
                WordTag::English => {
                    let (entry, sound, hyph) = aggregate_entries(eng_entries, false);
                    (vec![entry], sound, hyph)
                }
                WordTag::Both => {
                    let (mono_entry, mono_sound, mono_hyph) = aggregate_entries(mono_entries, true);
                    let (eng_entry, eng_sound, eng_hyph) = aggregate_entries(eng_entries, false);

                    // Prefer monolingual sound/hyphenation, fallback to English
                    let selected_sound = mono_sound.or(eng_sound);
                    let selected_hyphenation = mono_hyph.or(eng_hyph);

                    (
                        vec![mono_entry, eng_entry],
                        selected_sound,
                        selected_hyphenation,
                    )
                }
            };

            WordWithTaggedEntries {
                word,
                tag,
                entries,
                sounds: selected_sound,
                hyphenations: selected_hyphenation,
            }
        })
        .collect();

    result.sort_by(|a, b| a.word.cmp(&b.word));
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::DictionaryReader;
    use std::io::Cursor;
    use tarkka::{WordEntryComplete, WordTag};

    fn create_test_word(
        _word: &str,
        pos: &str,
        gloss: &str,
    ) -> (
        WordEntryComplete,
        Vec<tarkka::kaikki::Sound>,
        Vec<tarkka::Hyphenation>,
    ) {
        (
            WordEntryComplete {
                senses: vec![tarkka::Sense {
                    pos: pos.to_string(),
                    glosses: vec![tarkka::Gloss {
                        shared_prefix_count: 0,
                        new_categories: vec![],
                        gloss: gloss.to_string(),
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
        write_tagged(&mut buffer, tagged_words);

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
            word.entries[0].senses[0].glosses[0].gloss,
            "a book of word definitions"
        );
        assert_eq!(word.entries[1].senses[0].pos, "noun"); // Second entry is English
        assert_eq!(word.entries[1].senses[0].glosses[0].gloss, "reference book");

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
                        shared_prefix_count: 0,
                        new_categories: vec![],
                        gloss: "first noun definition".to_string(),
                    }],
                },
                tarkka::Sense {
                    pos: "adj".to_string(),
                    glosses: vec![tarkka::Gloss {
                        shared_prefix_count: 0,
                        new_categories: vec![],
                        gloss: "adjective definition".to_string(),
                    }],
                },
                tarkka::Sense {
                    pos: "noun".to_string(),
                    glosses: vec![tarkka::Gloss {
                        shared_prefix_count: 0,
                        new_categories: vec![],
                        gloss: "second noun definition".to_string(),
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
