use tarkka::{AggregatedWord, HEADER_SIZE, PosGlosses, WordEntry, WordEntryComplete};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, Write};
use std::time::Instant;

pub mod reader;

fn main() {
    let lang = "es";
    let good_words = match File::open(format!("filtered-{lang}-raw-wiktextract-data.jsonl")) {
        Ok(mut f) => {
            println!("parsing json from pre-filtered");
            let mut s = String::new();
            f.read_to_string(&mut s).unwrap();
            serde_json::from_str(s.as_str()).unwrap()
            // serde_json::from_reader(f).unwrap()
            // much slower??
        }
        Err(_) => {
            println!("filtered not found, creating");
            let f = File::open(format!("{lang}-raw-wiktextract-data.jsonl")).unwrap();
            let s = Instant::now();
            let good_words = filter(lang, f);
            println!("Filter took {:?}", s.elapsed());
            let s = Instant::now();
            {
                let mut f =
                    File::create(format!("filtered-{lang}-raw-wiktextract-data.jsonl")).unwrap();
                let ser = serde_json::to_string_pretty(&good_words).unwrap();
                f.write_all(ser.as_bytes()).unwrap();
            }
            println!("serialize took {:?}", s.elapsed());
            good_words
        }
    };

    let s = Instant::now();
    let words = build_index(good_words);
    println!("Build index took {:?}", s.elapsed());

    let s = Instant::now();
    let file = File::create(format!("{lang}-dictionary.dict")).unwrap();
    write(file, words);
    println!("writing took {:?}", s.elapsed());
}

fn common_prefix_len(a: &str, b: &str) -> usize {
    a.bytes().zip(b.bytes()).take_while(|(x, y)| x == y).count()
}

pub fn write<W: Write>(mut w: W, sorted_words: Vec<AggregatedWord>) {
    let s = Instant::now();
    let mut groups: BTreeMap<[u8; 3], Vec<&AggregatedWord>> = BTreeMap::new();
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

    let mut l2_dbg = Vec::new();

    for (l1_group, words) in groups {
        let mut l2_raw_size = 0u32;
        let mut prev_word = "";
        let group_binary_start = global_binary_offset;

        for word in words {
            let current_word = &word.word;
            let shared_len = common_prefix_len(prev_word, current_word);
            let suffix = &current_word.as_bytes()[shared_len..];

            shared_prefixes += shared_len;
            // Format: [shared_prefix_len(1b)][suffix_len 1b][suffix][size 2b]
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
            // encoding `entry_size` as a 2b LEB saves 70KiB on
            // the english dict, not worth the complexity

            let fixed_ovh = 1 + 1 + ser_size_b.len();
            debug_assert!(fixed_ovh <= 4);
            let entry_size = suffix.len() + fixed_ovh;
            if word.word == "dictionary" {
                println!("L1 says data for 'dic' starts at {group_binary_start}");
                println!("L2 says size is {}", ser_size);
                println!("L2 calculated data offset {}", global_binary_offset);
                println!(
                    "Actual serialized data so far ('dictionary' data starts at) {}",
                    all_serialized.len()
                );
            }
            all_serialized.extend_from_slice(&serialized);
            if l1_group == ['d' as u8, 'i' as u8, 'c' as u8] {
                l2_dbg.push(shared_len as u8);
                l2_dbg.push(suffix.len() as u8);
                l2_dbg.extend_from_slice(suffix);
                l2_dbg.extend_from_slice(&ser_size_b);
            }
            global_binary_offset += ser_size as u32;
            l2_raw_size += entry_size as u32;

            prev_word = current_word;
        }
        // ^ l2

        // Level 1: [3-byte prefix][l2 size u32][binary offset u32]
        level1_data.extend(l1_group);
        level1_data.extend(l2_raw_size.to_le_bytes());
        level1_data.extend(group_binary_start.to_le_bytes());

        level2_size += l2_raw_size as u32;
    }
    println!("saved {shared_prefixes}b with prefix thing");
    println!("compressed {:?}", s.elapsed());

    // Write all JSON data to encoder
    let mut total_ser_size = 0u32;
    encoder.write_all(&all_serialized).unwrap();
    total_ser_size += all_serialized.len() as u32;
    encoder.finish().unwrap();
    println!("finish compress {:?}", s.elapsed());

    w.write_all(b"DICT").unwrap(); // magic
    w.write_all(&(level1_data.len() as u32).to_le_bytes())
        .unwrap(); // level1 size
    w.write_all(&(level2_size as u32).to_le_bytes()).unwrap(); // level2 size
    w.write_all(&total_ser_size.to_le_bytes()).unwrap(); // json data size
    // ^^ header = 16b
    w.write_all(&level1_data).unwrap();
    w.write_all(&output).unwrap();

    let compressed_ser_sz = output.len() - level2_size as usize;

    println!("Created dictionary.dict with {} words", sorted_words.len());
    println!("Header size (static) {}", HEADER_SIZE); // DICT + l1 len + l2 size + ser size
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

fn filter<R: Read + Seek>(wanted_lang: &str, raw_data: R) -> Vec<WordEntryComplete> {
    let reader = BufReader::new(raw_data);
    let lines = reader.lines();
    let unwanted_pos = vec!["proverb"];
    let mut words: Vec<WordEntryComplete> = Vec::with_capacity(1_000_000);

    for line in lines {
        let line = line.unwrap();
        if line.len() == 0 {
            continue;
        }

        let word: WordEntry = serde_json::from_str(&line).unwrap();
        match word.lang_code {
            None => continue,
            Some(lang_code) => {
                if lang_code != wanted_lang {
                    continue;
                }
            }
        }
        match word.word {
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

        match word.pos {
            None => continue,
            Some(pos) => {
                if unwanted_pos.contains(&pos.as_str()) {
                    continue;
                }
            }
        }
        // ^^ parseable
        // vv parse
        let word: WordEntryComplete = serde_json::from_str(&line).unwrap();
        // no definitions, not the most useful dictionary
        if word.senses.iter().all(|s| s.glosses.is_none()) {
            continue;
        }
        words.push(word);
    }
    words
}

/// Returns SORTED
pub fn build_index(words: Vec<WordEntryComplete>) -> Vec<AggregatedWord> {
    let mut aggregated_words: HashMap<&str, AggregatedWord> = HashMap::with_capacity(1_000_000);
    let s = Instant::now();
    for i in 0..words.len() {
        let word = &words[i];
        // there are exactly 0 or 1 hyphenations
        // this was not true
        let hyphenation = if let Some(ref h) = word.hyphenations {
            // FIXME
            // println!("{h:?}");
            // assert!(h.len() <= 1);
            Some(h[0].parts.clone())
        } else {
            None
        };

        let mut all_form_of = Vec::new();

        // Create a temporary structure to hold category paths during processing
        struct TempGloss {
            category_path: Vec<String>,
            gloss: String,
        }

        let mut temp_glosses = Vec::new();
        for sense in &word.senses {
            if let Some(glosses) = &sense.glosses {
                if glosses.len() > 1 {
                    let category_path = glosses[0..glosses.len() - 1].to_vec();
                    let gloss = glosses[glosses.len() - 1].clone();
                    temp_glosses.push(TempGloss {
                        category_path,
                        gloss,
                    });
                } else if glosses.len() == 1 {
                    temp_glosses.push(TempGloss {
                        category_path: vec![],
                        gloss: glosses[0].clone(),
                    });
                }
            }
            if let Some(form_of) = &sense.form_of {
                all_form_of.extend(form_of.iter().map(|f| f.word.clone()));
            }
        }

        // Consolidate glosses: if we have an uncategorized gloss that matches a category,
        // convert the standalone gloss to be grouped under the category
        let mut consolidated_glosses = Vec::new();
        let mut used_indices = std::collections::HashSet::new();

        for (i, gloss) in temp_glosses.iter().enumerate() {
            if used_indices.contains(&i) {
                continue;
            }

            if gloss.category_path.is_empty() {
                // Check if this gloss text appears as a category in other entries
                let mut found_as_category = false;
                for (j, other_gloss) in temp_glosses.iter().enumerate() {
                    if i != j
                        && !other_gloss.category_path.is_empty()
                        && other_gloss.category_path[0] == gloss.gloss
                    {
                        found_as_category = true;
                        break;
                    }
                }

                if found_as_category {
                    // This standalone gloss should be converted to a category
                    // Don't add it as a standalone - it will be handled as a category header
                    used_indices.insert(i);

                    // Find all glosses that use this as a category and add them
                    for (j, other_gloss) in temp_glosses.iter().enumerate() {
                        if i != j
                            && !other_gloss.category_path.is_empty()
                            && other_gloss.category_path[0] == gloss.gloss
                        {
                            consolidated_glosses.push(TempGloss {
                                category_path: other_gloss.category_path.clone(),
                                gloss: other_gloss.gloss.clone(),
                            });
                            used_indices.insert(j);
                        }
                    }
                } else {
                    // Keep as uncategorized
                    consolidated_glosses.push(TempGloss {
                        category_path: gloss.category_path.clone(),
                        gloss: gloss.gloss.clone(),
                    });
                    used_indices.insert(i);
                }
            } else {
                // Check if this category already has a standalone version
                let mut has_standalone = false;
                for (j, other_gloss) in temp_glosses.iter().enumerate() {
                    if i != j
                        && other_gloss.category_path.is_empty()
                        && other_gloss.gloss == gloss.category_path[0]
                    {
                        has_standalone = true;
                        break;
                    }
                }

                if !has_standalone {
                    // Add this categorized gloss normally
                    consolidated_glosses.push(TempGloss {
                        category_path: gloss.category_path.clone(),
                        gloss: gloss.gloss.clone(),
                    });
                    used_indices.insert(i);
                }
                // If it has a standalone version, it will be handled above
            }
        }

        // Do not sort consolidated_glosses; they are supposed to come in order
        // and there's _some amount_ of relevancy to the order in wiktionary
        // Convert to compressed format using prefix compression
        let mut processed_glosses = Vec::new();
        let mut last_category_path: Vec<String> = Vec::new();

        for consolidated_gloss in consolidated_glosses {
            // Find common prefix with previous gloss
            let mut shared_count = 0;
            while shared_count < last_category_path.len()
                && shared_count < consolidated_gloss.category_path.len()
                && last_category_path[shared_count]
                    == consolidated_gloss.category_path[shared_count]
            {
                shared_count += 1;
            }

            // New categories are the ones after the shared prefix
            let new_categories = consolidated_gloss.category_path[shared_count..].to_vec();

            processed_glosses.push(tarkka::Gloss {
                shared_prefix_count: shared_count as u8,
                new_categories,
                gloss: consolidated_gloss.gloss,
            });

            // Update last category path for next iteration
            last_category_path = consolidated_gloss.category_path;
        }

        let ipa_sound = if let Some(sounds) = &word.sounds {
            let ipa_strings: Vec<String> = sounds.iter().filter_map(|s| s.ipa.clone()).collect();
            if ipa_strings.is_empty() {
                None
            } else {
                // TODO: dedup
                Some(ipa_strings)
            }
        } else {
            None
        };

        let form_of = if all_form_of.is_empty() {
            None
        } else {
            Some(all_form_of)
        };

        aggregated_words
            .entry(&word.word)
            .and_modify(|agg| {
                if let Some(existing_pos) = agg.pos_glosses.iter_mut().find(|pg| pg.pos == word.pos)
                {
                    if (existing_pos.glosses.len() + processed_glosses.len()) >= 256 {
                        println!("WTF? extend {word:?}");
                    } else {
                        existing_pos.glosses.extend(processed_glosses.clone());
                    }
                } else {
                    if processed_glosses.len() > 256 {
                        println!("WTF? push {word:?}");
                    } else {
                        agg.pos_glosses.push(PosGlosses {
                            pos: word.pos.clone(),
                            glosses: processed_glosses.clone(),
                        });
                    }
                }
                if let Some(form_of) = &form_of {
                    if let Some(existing_form_of) = &mut agg.form_of {
                        existing_form_of.extend(form_of.clone());
                    } else {
                        agg.form_of = Some(form_of.clone());
                    }
                }
                if let Some(ipa_sound) = &ipa_sound {
                    if let Some(existing_ipa_sound) = &mut agg.ipa_sound {
                        existing_ipa_sound.extend(ipa_sound.clone());
                    } else {
                        agg.ipa_sound = Some(ipa_sound.clone());
                    }
                }
            })
            .or_insert(AggregatedWord {
                word: word.word.clone(),
                pos_glosses: vec![PosGlosses {
                    pos: word.pos.clone(),
                    glosses: processed_glosses[..std::cmp::min(255, processed_glosses.len())]
                        .to_vec(),
                }],
                hyphenation,
                form_of,
                ipa_sound,
            });
    }
    println!("stage 1 {:?}", s.elapsed());
    let s = Instant::now();

    // some verbs have a `form_of` referencing a word that's not yet on the dictionary
    // remove it. presenting a 404 link to the user is terrible
    let word_keys: HashSet<&str> = aggregated_words.keys().copied().collect();
    for aggregated_word in aggregated_words.values_mut() {
        if let Some(form_of) = &mut aggregated_word.form_of {
            form_of.retain(|word| word_keys.contains(word.as_str()));
            if form_of.is_empty() {
                aggregated_word.form_of = None;
            }
        }
    }
    println!("stage 2 {:?}", s.elapsed());

    let s = Instant::now();
    let mut ret: Vec<AggregatedWord> = aggregated_words.into_values().collect();
    ret.sort_by(|a, b| a.word.cmp(&b.word));
    println!("sorted {:?}", s.elapsed());
    ret
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::DictionaryReader;
    use crate::{Sense, WordEntryComplete};
    use std::io::Cursor;

    fn create_test_word(word: &str, pos: &str, gloss: &str) -> WordEntryComplete {
        WordEntryComplete {
            word: word.to_string(),
            pos: pos.to_string(),
            senses: vec![Sense {
                glosses: Some(vec![gloss.to_string()]),
                form_of: None,
            }],
            hyphenations: None,
            sounds: None,
        }
    }

    #[test]
    fn test_build_index_with_prefix_cases() {
        let test_words = vec![
            create_test_word("dictate", "verb", "to say words aloud"),
            create_test_word("dictionary", "noun", "a book of word definitions"),
            create_test_word("dictoto", "noun", "fictional word for testing"),
            create_test_word("pa", "noun", "short word"),
            create_test_word("papa", "noun", "father"),
            create_test_word("papo", "noun", "chat"),
            create_test_word("potato", "noun", "a vegetable"),
        ];

        let result = build_index(test_words);

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

        for aggregated_word in &result {
            assert!(!aggregated_word.pos_glosses.is_empty());
            assert!(!aggregated_word.pos_glosses[0].glosses.is_empty());
            assert!(!aggregated_word.pos_glosses[0].glosses[0].gloss.is_empty());
        }
    }

    #[test]
    fn test_build_index_write_read_roundtrip() {
        let test_words = vec![
            create_test_word("dictate", "verb", "to say words aloud"),
            create_test_word("dictionary", "noun", "a book of word definitions"),
            create_test_word("dictoto", "noun", "fictional word for testing"),
            create_test_word("pa", "noun", "short word"),
            create_test_word("papa", "noun", "father"),
            create_test_word("papo", "noun", "chat"),
            create_test_word("potato", "noun", "a vegetable"),
        ];

        let aggregated_words = build_index(test_words);

        let mut buffer = Vec::new();
        write(&mut buffer, aggregated_words);

        let cursor = Cursor::new(buffer);
        let mut dict_reader = DictionaryReader::open(cursor).unwrap();

        let result = dict_reader.lookup("dictionary").unwrap();
        assert!(result.is_some());
        let word = result.unwrap();
        assert_eq!(word.word, "dictionary");
        assert_eq!(word.pos_glosses[0].pos, "noun");
        assert_eq!(
            word.pos_glosses[0].glosses[0].gloss,
            "a book of word definitions"
        );

        let result = dict_reader.lookup("papa").unwrap();
        assert!(result.is_some());
        let word = result.unwrap();
        assert_eq!(word.word, "papa");
        assert_eq!(word.pos_glosses[0].pos, "noun");
        assert_eq!(word.pos_glosses[0].glosses[0].gloss, "father");

        let result = dict_reader.lookup("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_category_extraction() {
        let dog_word = WordEntryComplete {
            word: "dog".to_string(),
            pos: "noun".to_string(),
            senses: vec![
                Sense {
                    glosses: Some(vec![
                        "A mammal of the family Canidae:".to_string(),
                        "The species Canis familiaris, domesticated for thousands of years.".to_string(),
                    ]),
                    form_of: None,
                },
                Sense {
                    glosses: Some(vec![
                        "A mammal of the family Canidae:".to_string(),
                        "Any member of the family Canidae, including domestic dogs, wolves, coyotes.".to_string(),
                    ]),
                    form_of: None,
                },
            ],
            hyphenations: None,
            sounds: None,
        };

        let test_words = vec![dog_word];
        let result = build_index(test_words);

        assert_eq!(result.len(), 1);
        let word = &result[0];
        assert_eq!(word.word, "dog");
        assert_eq!(word.pos_glosses[0].glosses.len(), 2);

        let first_gloss = &word.pos_glosses[0].glosses[0];
        // First gloss should have no shared prefix and full category in new_categories
        assert_eq!(first_gloss.shared_prefix_count, 0);
        assert_eq!(
            first_gloss.new_categories,
            vec!["A mammal of the family Canidae:"]
        );
        assert_eq!(
            first_gloss.gloss,
            "The species Canis familiaris, domesticated for thousands of years."
        );

        let second_gloss = &word.pos_glosses[0].glosses[1];
        // Second gloss should share the category prefix with first gloss
        assert_eq!(second_gloss.shared_prefix_count, 1); // Shares 1 category with previous
        assert_eq!(second_gloss.new_categories, Vec::<String>::new()); // No new categories
        assert_eq!(
            second_gloss.gloss,
            "Any member of the family Canidae, including domestic dogs, wolves, coyotes."
        );
    }

    #[test]
    fn test_hierarchical_categories() {
        let place_word = WordEntryComplete {
            word: "denmark".to_string(),
            pos: "noun".to_string(),
            senses: vec![
                Sense {
                    glosses: Some(vec![
                        "A number of places in other countries:".to_string(),
                        "town in Western Australia".to_string(),
                    ]),
                    form_of: None,
                },
                Sense {
                    glosses: Some(vec![
                        "A number of places in other countries:".to_string(),
                        "community in Nova Scotia".to_string(),
                    ]),
                    form_of: None,
                },
                Sense {
                    glosses: Some(vec![
                        "A number of places in other countries:".to_string(),
                        "{{place|en|place|c/USA}}:".to_string(),
                        "community in Georgia".to_string(),
                    ]),
                    form_of: None,
                },
                Sense {
                    glosses: Some(vec![
                        "A number of places in other countries:".to_string(),
                        "{{place|en|place|c/USA}}:".to_string(),
                        "community in Indiana".to_string(),
                    ]),
                    form_of: None,
                },
            ],
            hyphenations: None,
            sounds: None,
        };

        let test_words = vec![place_word];
        let result = build_index(test_words);

        assert_eq!(result.len(), 1);
        let word = &result[0];
        assert_eq!(word.word, "denmark");
        assert_eq!(word.pos_glosses[0].glosses.len(), 4);

        // Test prefix compression in hierarchical categories
        let gloss0 = &word.pos_glosses[0].glosses[0];
        assert_eq!(gloss0.shared_prefix_count, 0);
        assert_eq!(
            gloss0.new_categories,
            vec!["A number of places in other countries:"]
        );
        assert_eq!(gloss0.gloss, "town in Western Australia");

        let gloss1 = &word.pos_glosses[0].glosses[1];
        assert_eq!(gloss1.shared_prefix_count, 1); // Shares 1 category with gloss0
        assert_eq!(gloss1.new_categories, Vec::<String>::new());
        assert_eq!(gloss1.gloss, "community in Nova Scotia");

        // Nested categories
        let gloss2 = &word.pos_glosses[0].glosses[2];
        assert_eq!(gloss2.shared_prefix_count, 1); // Shares 1 category with previous
        assert_eq!(gloss2.new_categories, vec!["{{place|en|place|c/USA}}:"]);
        assert_eq!(gloss2.gloss, "community in Georgia");

        let gloss3 = &word.pos_glosses[0].glosses[3];
        assert_eq!(gloss3.shared_prefix_count, 2); // Shares 2 categories with gloss2
        assert_eq!(gloss3.new_categories, Vec::<String>::new());
        assert_eq!(gloss3.gloss, "community in Indiana");
    }

    #[test]
    fn test_category_sorting_and_grouping() {
        let deer_word = WordEntryComplete {
            word: "deer".to_string(),
            pos: "noun".to_string(),
            senses: vec![
                Sense {
                    glosses: Some(vec![
                        "A ruminant mammal with hooves and often antlers, of the family Cervidae."
                            .to_string(),
                    ]),
                    form_of: None,
                },
                Sense {
                    glosses: Some(vec!["The meat of such an animal; venison.".to_string()]),
                    form_of: None,
                },
                Sense {
                    glosses: Some(vec![
                        "A ruminant mammal with hooves and often antlers, of the family Cervidae."
                            .to_string(),
                        "One of the smaller animals of the family Cervidae.".to_string(),
                    ]),
                    form_of: None,
                },
                Sense {
                    glosses: Some(vec![
                        "Any animal, especially a quadrupedal mammal.".to_string(),
                    ]),
                    form_of: None,
                },
            ],
            hyphenations: None,
            sounds: None,
        };

        let test_words = vec![deer_word];
        let result = build_index(test_words);

        assert_eq!(result.len(), 1);
        let word = &result[0];
        assert_eq!(word.word, "deer");
        // After consolidation, we should have 3 glosses:
        // - Two uncategorized glosses
        // - One categorized gloss (the standalone "A ruminant mammal..." was consolidated into the category)
        assert_eq!(word.pos_glosses[0].glosses.len(), 3);

        // Verify the consolidation and compression worked correctly:
        let gloss0 = &word.pos_glosses[0].glosses[0];
        assert_eq!(gloss0.shared_prefix_count, 0);
        assert_eq!(
            gloss0.new_categories,
            vec!["A ruminant mammal with hooves and often antlers, of the family Cervidae."]
        );
        assert_eq!(
            gloss0.gloss,
            "One of the smaller animals of the family Cervidae."
        );

        let gloss1 = &word.pos_glosses[0].glosses[1];
        assert_eq!(gloss1.shared_prefix_count, 0); // No shared categories (uncategorized)
        assert_eq!(gloss1.new_categories, Vec::<String>::new());
        assert_eq!(gloss1.gloss, "The meat of such an animal; venison.");

        let gloss2 = &word.pos_glosses[0].glosses[2];
        assert_eq!(gloss2.shared_prefix_count, 0); // No shared categories (uncategorized)
        assert_eq!(gloss2.new_categories, Vec::<String>::new());
        assert_eq!(gloss2.gloss, "Any animal, especially a quadrupedal mammal.");
    }
}
