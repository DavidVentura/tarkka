use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Write;

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

#[derive(Debug, Serialize)]
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

    // the dictionary is JSONL
    // the index is [<word>\0<u32 offset>]
    let mut json_data = String::new();
    let mut index_data = Vec::with_capacity(16 * 1024 * 1024); // 16MB index is a reasonable
    // starting point
    let mut current_offset: u32 = 0;

    let mut sorted_words: Vec<_> = aggregated_words.values().collect();
    sorted_words.sort_by(|a, b| a.word.cmp(&b.word));

    for aggregated_word in sorted_words {
        index_data.extend(aggregated_word.word.as_bytes());
        index_data.push(0);
        index_data.extend(u32::to_le_bytes(current_offset));

        let serialized = serde_json::to_string(&aggregated_word).unwrap();
        json_data.push_str(&serialized);
        json_data.push('\n');

        current_offset += serialized.len() as u32 + 1; // +1 for newline
    }

    let compressed_data = zstd::encode_all(json_data.as_bytes(), 9).unwrap();

    let mut file = File::create("dictionary.dict").unwrap();
    let mut file2 = File::create("dictionary.zstd").unwrap();
    let mut idx = File::create("index").unwrap();

    file.write_all(b"DICT").unwrap(); // magic
    file.write_all(&(index_data.len() as u32).to_le_bytes())
        .unwrap();
    file.write_all(&(compressed_data.len() as u32).to_le_bytes())
        .unwrap();

    file.write_all(&index_data).unwrap();
    idx.write_all(&index_data).unwrap();

    file.write_all(&compressed_data).unwrap();
    file2.write_all(&compressed_data).unwrap();

    println!(
        "Created dictionary.dict with {} words",
        aggregated_words.len()
    );
    println!("Original JSON size: {} bytes", json_data.len());
    println!(
        "Compressed size: {} bytes ({:.1}x compression)",
        compressed_data.len(),
        (json_data.len() as f64 / compressed_data.len() as f64) * 100.0
    );
}
