use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::Write;
use tarkka::{AggregatedWord, PosGlosses, WordEntry, WordEntryComplete};

fn main() {
    let words = build_index();
    write(words);
}

fn write(aggregated_words: HashMap<String, AggregatedWord>) {
    let mut sorted_words: Vec<_> = aggregated_words.values().collect();
    sorted_words.sort_by(|a, b| a.word.cmp(&b.word));

    let mut json_data = String::new();
    let mut word_to_offset: HashMap<String, u32> = HashMap::new();
    let mut current_offset: u32 = 0;

    for word in &sorted_words {
        // TODO straight into encoder
        word_to_offset.insert(word.word.clone(), current_offset);
        let serialized = serde_json::to_string(word).unwrap();
        json_data.push_str(&serialized);
        current_offset += serialized.len() as u32;
    }

    let mut groups: BTreeMap<String, Vec<&AggregatedWord>> = BTreeMap::new();
    for word in &sorted_words {
        let first_char = word.word.chars().next().unwrap().to_string();
        groups.entry(first_char).or_default().push(word);
    }

    let mut level1_data = Vec::with_capacity(1 * 1024 * 1024);

    let mut output = Vec::with_capacity(16 * 1024 * 1024);
    let mut encoder = zeekstd::Encoder::new(&mut output).unwrap();
    let mut level2_size: u32 = 0;

    for (first_char, words) in groups {
        let mut l2_raw_size = 0u32;
        for word in words {
            let json_offset = word_to_offset[&word.word];
            let serialized = serde_json::to_string(word).unwrap();
            // TODO: move this to the front of the json entry to avoid serializing twice
            let json_size = serialized.len();

            assert!(
                json_size <= 65535,
                "JSON entry too large: {} bytes for word '{}'",
                json_size,
                word.word
            );

            // Format: [word_len][word][offset_in_json_data][json_size]
            // println!("w {} jo {} js {}", word.word, json_offset, json_size);

            assert!(word.word.len() <= 255);
            encoder.write(&[word.word.len() as u8]).unwrap();
            encoder.write(word.word.as_bytes()).unwrap();
            encoder.write(json_offset.to_le_bytes().as_slice()).unwrap();
            encoder
                .write((json_size as u16).to_le_bytes().as_slice())
                .unwrap();
            l2_raw_size += 1 + word.word.len() as u32 + 4 + 2;
        }

        println!("c {first_char} l2off {level2_size} l2sz {l2_raw_size}");
        // Level 1: [first_char_utf8_bytes][l2 offset u32][l2 size u32]
        level1_data.extend(first_char.as_bytes());
        level1_data.extend(level2_size.to_le_bytes());
        level1_data.extend(l2_raw_size.to_le_bytes());

        level2_size += l2_raw_size;
    }

    println!("l2 sz {}", level2_size);
    let jb = json_data.as_bytes();
    encoder.write_all(jb).unwrap();
    encoder.finish().unwrap();

    let mut file = File::create("dictionary.dict").unwrap();

    file.write_all(b"DICT").unwrap(); // magic
    file.write_all(&(level1_data.len() as u32).to_le_bytes())
        .unwrap(); // level1 size
    file.write_all(&(level2_size as u32).to_le_bytes()).unwrap(); // level2 size
    file.write_all(&(jb.len() as u32).to_le_bytes()).unwrap(); // json data size

    file.write_all(&level1_data).unwrap();
    file.write_all(&output).unwrap();

    println!(
        "Created dictionary.dict with {} words",
        aggregated_words.len()
    );
    println!("Header size (static) {}", 4 + 4 + 4 + 4); // DICT + l1 len + l2 size + json size
    println!("Level 1 size: {} bytes", level1_data.len());
    println!("Level 2 size: {} bytes", level2_size);
    println!("Raw JSON data size: {}", json_data.len());
}

fn build_index() -> HashMap<String, AggregatedWord> {
    //let content = std::fs::read_to_string("./example-es.jsonl").unwrap();
    let content = std::fs::read_to_string("./es-extract.jsonl").unwrap();
    let lines = content.split('\n');
    let unwanted_pos = vec!["proverb"];
    let wanted_lang = "es";
    let mut aggregated_words: HashMap<String, AggregatedWord> = HashMap::new();

    for line in lines {
        if line.len() == 0 {
            continue;
        }

        let word: WordEntry = serde_json::from_str(line).unwrap();
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
                if w.contains(" ") {
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
        // println!("{line}");
        let word: WordEntryComplete = serde_json::from_str(line).unwrap();
        // no definitions, not the most useful dictionary
        if word.senses.iter().all(|s| s.glosses.is_none()) {
            continue;
        }

        // there are exactly 0 or 1 hyphenations
        let hyphenation = if let Some(h) = word.hyphenations {
            assert!(h.len() <= 1);
            Some(h[0].parts.clone())
        } else {
            None
        };

        let mut all_glosses = Vec::new();
        let mut all_form_of = Vec::new();
        for sense in &word.senses {
            if let Some(glosses) = &sense.glosses {
                all_glosses.extend(glosses.clone());
            }
            if let Some(form_of) = &sense.form_of {
                all_form_of.extend(form_of.iter().map(|f| f.word.clone()));
            }
        }

        let ipa_sound = if let Some(sounds) = &word.sounds {
            let ipa_strings: Vec<String> = sounds.iter().filter_map(|s| s.ipa.clone()).collect();
            if ipa_strings.is_empty() {
                None
            } else {
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
            .entry(word.word.clone())
            .and_modify(|agg| {
                if let Some(existing_pos) = agg.pos_glosses.iter_mut().find(|pg| pg.pos == word.pos)
                {
                    existing_pos.glosses.extend(all_glosses.clone());
                } else {
                    agg.pos_glosses.push(PosGlosses {
                        pos: word.pos.clone(),
                        glosses: all_glosses.clone(),
                    });
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
                    pos: word.pos,
                    glosses: all_glosses,
                }],
                hyphenation,
                form_of,
                ipa_sound,
            });
    }

    // some verbs have a `form_of` referencing a word that's not yet on the dictionary
    // remove it. presenting a 404 link to the user is terrible
    let word_keys: HashSet<String> = aggregated_words.keys().cloned().collect();
    for aggregated_word in aggregated_words.values_mut() {
        if let Some(form_of) = &mut aggregated_word.form_of {
            form_of.retain(|word| word_keys.contains(word));
            if form_of.is_empty() {
                aggregated_word.form_of = None;
            }
        }
    }

    aggregated_words
}
