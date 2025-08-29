use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, Write};
use std::time::Instant;
use tarkka::{AggregatedWord, PosGlosses, WordEntry, WordEntryComplete};

fn main() {
    let lang = "en";
    let good_words = match File::open(format!("filtered-{lang}-raw-wiktextract-data.jsonl")) {
        Ok(mut f) => {
            println!("parsing json from pre-filtered");
            let mut s = String::new();
            f.read_to_string(&mut s).unwrap();
            serde_json::from_str(s.as_str()).unwrap()
            // serde_json::from_reader(f).unwrap()
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
                let ser = serde_json::to_string(&good_words).unwrap();
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
    a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count()
}

fn split_at_char_boundary(s: &str, char_index: usize) -> &str {
    if char_index == 0 {
        return s;
    }
    let mut char_indices = s.char_indices();
    for _ in 0..char_index {
        if char_indices.next().is_none() {
            return s;
        }
    }
    if let Some((byte_index, _)) = char_indices.next() {
        &s[byte_index..]
    } else {
        ""
    }
}

fn write(mut file: File, sorted_words: Vec<AggregatedWord>) {
    let mut words_with_json: Vec<(&AggregatedWord, String)> =
        Vec::with_capacity(sorted_words.len());
    let s = Instant::now();
    for word in &sorted_words {
        let serialized = serde_json::to_string(word).unwrap();
        words_with_json.push((word, serialized));
    }
    println!("serialized {:?}", s.elapsed());

    let mut groups: BTreeMap<String, Vec<(&AggregatedWord, u16)>> = BTreeMap::new();
    let mut word_size = 0;
    for (word, json) in &words_with_json {
        let first_char = word.word.chars().next().unwrap().to_string();
        word_size += word.word.len();
        assert!(
            json.len() <= u16::MAX as usize,
            "JSON entry too large: {} bytes for word '{}'",
            json.len(),
            word.word
        );
        groups
            .entry(first_char)
            .or_default()
            .push((word, json.len() as u16));
    }

    for group in groups.values_mut() {
        group.sort_by(|a, b| a.0.word.cmp(&b.0.word));
    }
    println!("grouped {:?}", s.elapsed());
    println!("total word len {word_size}");

    let mut level1_data = Vec::with_capacity(4096);

    let mut output = Vec::with_capacity(32 * 1024 * 1024);
    let mut encoder = zeekstd::Encoder::new(&mut output).unwrap();
    let mut level2_size: u32 = 0;

    let mut max_jo = 0;
    let mut current_json_offset: u32 = 0;

    let mut shared_prefixes = 0;

    for (first_char, words) in groups {
        let mut l2_raw_size = 0u32;
        let mut restart_word = "";
        let mut word_count = 0u32;

        for (word, json_size) in words {
            let current_word = &word.word;
            let shared_len = if word_count == 0 {
                0
            } else {
                common_prefix_len(restart_word, current_word)
            };

            let needs_restart = shared_len < 4 || word_count == 0;
            let restart_flag = if needs_restart { 0x80u8 } else { 0x00u8 };
            let suffix = split_at_char_boundary(current_word, shared_len);

            shared_prefixes += shared_len;
            // Format: [restart_flag(1bit) + shared_prefix_len(7bits)][suffix_len][suffix][offset][size]
            assert!(shared_len <= 127, "Shared prefix too long: {}", shared_len);
            assert!(suffix.len() <= 255, "Suffix too long: {}", suffix.len());

            encoder.write(&[restart_flag | (shared_len as u8)]).unwrap();
            encoder.write(&[suffix.len() as u8]).unwrap();
            encoder.write(suffix.as_bytes()).unwrap();
            encoder
                .write(current_json_offset.to_le_bytes().as_slice())
                .unwrap();
            encoder
                .write((json_size as u16).to_le_bytes().as_slice())
                .unwrap();

            max_jo = std::cmp::max(max_jo, current_json_offset);
            current_json_offset = current_json_offset.checked_add(json_size as u32).unwrap();
            l2_raw_size += 1 + 1 + suffix.len() as u32 + 4 + 2;

            // Update restart_word when we have a restart
            if needs_restart {
                restart_word = current_word;
            }
            word_count += 1;
        }

        // Level 1: [first_char_utf8_bytes][l2 offset u32][l2 size u32]
        level1_data.extend(first_char.as_bytes());
        level1_data.extend(level2_size.to_le_bytes());
        level1_data.extend(l2_raw_size.to_le_bytes());

        level2_size += l2_raw_size;
    }
    println!("saved {shared_prefixes}b with prefix thing");
    println!("compressed {:?}", s.elapsed());

    println!("max_jo {max_jo} l2 sz {}", level2_size);

    // Write all JSON data to encoder
    let mut total_json_size = 0u32;
    for (_, serialized_json) in &words_with_json {
        encoder.write_all(serialized_json.as_bytes()).unwrap();
        total_json_size += serialized_json.len() as u32;
    }
    encoder.finish().unwrap();
    println!("finish compress {:?}", s.elapsed());

    file.write_all(b"DICT").unwrap(); // magic
    file.write_all(&(level1_data.len() as u32).to_le_bytes())
        .unwrap(); // level1 size
    file.write_all(&(level2_size as u32).to_le_bytes()).unwrap(); // level2 size
    file.write_all(&total_json_size.to_le_bytes()).unwrap(); // json data size

    file.write_all(&level1_data).unwrap();
    file.write_all(&output).unwrap();

    let compressed_json_sz = output.len() - level2_size as usize;

    println!("Created dictionary.dict with {} words", sorted_words.len());
    println!("Header size (static) {}", 4 + 4 + 4 + 4); // DICT + l1 len + l2 size + json size
    println!("Level 1 size: {} bytes", level1_data.len());
    println!("Level 2 size: {} bytes", level2_size);
    println!(
        "JSON data size: raw {} compressed {}",
        total_json_size, compressed_json_sz
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
fn build_index(words: Vec<WordEntryComplete>) -> Vec<AggregatedWord> {
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
                    pos: word.pos.clone(),
                    glosses: all_glosses,
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
