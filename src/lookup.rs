use std::{fs::File, time::Instant};

use tarkka::{WordTag, WordWithTaggedEntries, reader::DictionaryReader};

fn pretty_print(wn: &str, w: WordWithTaggedEntries) {
    // Collect all IPA pronunciations and hyphenations from all entries
    let mut all_ipa = Vec::new();
    let mut all_hyphenations = Vec::new();

    for entry in &w.entries {
        if let Some(sounds) = &entry.sounds {
            for sound in sounds {
                if let Some(ipa) = &sound.ipa {
                    if !all_ipa.contains(ipa) {
                        all_ipa.push(ipa.clone());
                    }
                }
            }
        }

        if let Some(hyphenations) = &entry.hyphenations {
            for hyphenation in hyphenations {
                let hyph_str = hyphenation.parts.join("-");
                if !all_hyphenations.contains(&hyph_str) {
                    all_hyphenations.push(hyph_str);
                }
            }
        }
    }

    // Display word with pronunciation and hyphenation
    let ipa_str = if all_ipa.is_empty() {
        "".to_string()
    } else {
        all_ipa.join(", ")
    };
    let hyph_str = if all_hyphenations.is_empty() {
        "".to_string()
    } else {
        all_hyphenations.join(", ")
    };

    println!("{wn} - {} - {}", ipa_str, hyph_str);

    // Display entries based on tag
    match w.tag {
        WordTag::Monolingual => {
            // All entries are monolingual
            for entry in &w.entries {
                for sense in &entry.senses {
                    println!("[MONO] {}:", sense.pos);
                    if let Some(glosses) = &sense.glosses {
                        for gloss in glosses {
                            println!("  - {}", gloss);
                        }
                    }
                }
            }
        }
        WordTag::English => {
            // All entries are English
            for entry in &w.entries {
                for sense in &entry.senses {
                    println!("[ENG] {}:", sense.pos);
                    if let Some(glosses) = &sense.glosses {
                        for gloss in glosses {
                            println!("  - {}", gloss);
                        }
                    }
                }
            }
        }
        WordTag::Both => {
            // Exactly 2 entries: first is monolingual, second is English
            assert_eq!(w.entries.len(), 2, "Both tag must have exactly 2 entries");

            // First entry is monolingual
            for sense in &w.entries[0].senses {
                println!("[MONO] {}:", sense.pos);
                if let Some(glosses) = &sense.glosses {
                    for gloss in glosses {
                        println!("  - {}", gloss);
                    }
                }
            }

            // Second entry is English
            for sense in &w.entries[1].senses {
                println!("[ENG] {}:", sense.pos);
                if let Some(glosses) = &sense.glosses {
                    for gloss in glosses {
                        println!("  - {}", gloss);
                    }
                }
            }
        }
    }
}

fn main() {
    let s = Instant::now();
    //let f = File::open("en-dictionary.dict").unwrap();
    let f = File::open("es-multi-dictionary.dict").unwrap();
    let mut d = DictionaryReader::open(f).unwrap();
    println!("read {:?}", s.elapsed());
    let s = Instant::now();
    let lookup = "perro";
    let r = d.lookup(lookup).unwrap();
    println!("looked 1st up {:?}", s.elapsed());
    if let Some(w) = r {
        // println!("{w:#?}");
        pretty_print(lookup, w);
    } else {
        println!("not found: '{lookup}'")
    };
}
