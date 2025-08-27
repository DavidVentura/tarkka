use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::Write;
use std::time::Instant;
mod reader;

#[derive(Debug, Deserialize, Serialize)]
struct WordEntry {
    pos: Option<String>,
    lang_code: Option<String>,
}
#[derive(Debug, Deserialize, Serialize)]
struct WordEntryComplete {
    pos: String,
    word: String,
    senses: Vec<Sense>,
    hyphenations: Option<Vec<Hyphenation>>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Sense {
    form_of: Option<Vec<FormOf>>,
    glosses: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize)]
struct FormOf {
    word: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct Hyphenation {
    parts: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct AggregatedWord {
    word: String,
    pos_glosses: Vec<PosGlosses>,
    hyphenation: Option<Vec<String>>,
    form_of: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize)]
struct PosGlosses {
    pos: String,
    glosses: Vec<String>,
}

fn main() {
    //write();
    let s = Instant::now();
    let mut d = reader::DictionaryReader::open("dictionary.dict").unwrap();
    println!("{:?}", s.elapsed());
    let r = d.lookup("perro").unwrap();
    println!("{:?}", s.elapsed());
    println!("word = {r:?}");
    let r = d.lookup("hola").unwrap();
    println!("{:?}", s.elapsed());
    println!("word = {r:?}");
}

fn write(aggregated_words: HashMap<String, AggregatedWord>) {
    let mut sorted_words: Vec<_> = aggregated_words.values().collect();
    sorted_words.sort_by(|a, b| a.word.cmp(&b.word));

    let mut json_data = String::new();
    let mut word_to_offset: HashMap<String, u32> = HashMap::new();
    let mut current_offset: u32 = 0;

    for word in &sorted_words {
        word_to_offset.insert(word.word.clone(), current_offset);
        let serialized = serde_json::to_string(word).unwrap();
        json_data.push_str(&serialized);
        json_data.push('\n');
        current_offset += serialized.len() as u32 + 1;
    }

    let mut groups: BTreeMap<String, Vec<&AggregatedWord>> = BTreeMap::new();
    for word in &sorted_words {
        let first_char = word.word.chars().next().unwrap().to_string();
        groups.entry(first_char).or_default().push(word);
    }

    let mut level1_data = Vec::new();
    let mut level2_data = Vec::new();
    let mut current_level2_offset: u32 = 0;

    for (first_char, words) in groups {
        let mut group_data = Vec::new();
        for word in words {
            // Format: [word_len][word][offset_in_json_data]
            group_data.push(word.word.len() as u8);
            group_data.extend(word.word.as_bytes());
            let json_offset = word_to_offset[&word.word];
            group_data.extend(json_offset.to_le_bytes());
        }

        let compressed_group = zstd::encode_all(group_data.as_slice(), 9).unwrap();

        // Level 1: [first_char_utf8_bytes][l2 offset u32][l2 size u32]
        level1_data.extend(first_char.as_bytes());
        level1_data.extend(current_level2_offset.to_le_bytes());
        level1_data.extend((compressed_group.len() as u32).to_le_bytes());

        current_level2_offset += compressed_group.len() as u32;
        level2_data.extend(compressed_group);
    }

    let compressed_json = zstd::encode_all(json_data.as_bytes(), 9).unwrap();

    let mut file = File::create("dictionary.dict").unwrap();

    file.write_all(b"DICT").unwrap(); // magic
    file.write_all(&(level1_data.len() as u32).to_le_bytes())
        .unwrap(); // level1 size
    file.write_all(&(level2_data.len() as u32).to_le_bytes())
        .unwrap(); // level2 size
    file.write_all(&(compressed_json.len() as u32).to_le_bytes())
        .unwrap(); // json data size

    file.write_all(&level1_data).unwrap();
    file.write_all(&level2_data).unwrap();
    file.write_all(&compressed_json).unwrap();

    println!(
        "Created dictionary.dict with {} words",
        aggregated_words.len()
    );
    println!("Level 1 size: {} bytes", level1_data.len());
    println!("Level 2 size: {} bytes", level2_data.len());
    println!(
        "JSON data size: {} bytes (compressed from {})",
        compressed_json.len(),
        json_data.len()
    );
    println!(
        "Total file size: {} bytes",
        16 + level1_data.len() + level2_data.len() + compressed_json.len()
    );
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
            })
            .or_insert(AggregatedWord {
                word: word.word.clone(),
                pos_glosses: vec![PosGlosses {
                    pos: word.pos,
                    glosses: all_glosses,
                }],
                hyphenation,
                form_of,
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
