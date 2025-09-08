use std::{fs::File, io::BufReader, time::Instant};

use tarkka::{WordTag, WordWithTaggedEntries, reader::DictionaryReader};

fn display_glosses_with_categories(glosses: &[tarkka::Gloss], pos: &str, tag_prefix: &str) {
    let mut last_category_path: Vec<String> = Vec::new();

    println!("{}{}: ", tag_prefix, pos);

    for gloss in glosses {
        // Reconstruct full category path from compressed format
        let mut current_category_path = Vec::new();

        // Add shared prefix from previous gloss
        /*
        if gloss.shared_prefix_count > 0 {
            let prefix_len = (gloss.shared_prefix_count as usize).min(last_category_path.len());
            current_category_path.extend_from_slice(&last_category_path[..prefix_len]);
        }

        // Add new categories
        if !gloss.new_categories.is_empty() {
            current_category_path.extend(gloss.new_categories.clone());
        }
        */

        // Find how much of the category path has changed
        let mut common_len = 0;
        while common_len < last_category_path.len()
            && common_len < current_category_path.len()
            && last_category_path[common_len] == current_category_path[common_len]
        {
            common_len += 1;
        }

        // Print any new category levels
        for (i, category) in current_category_path.iter().enumerate().skip(common_len) {
            let indent = "  ".repeat(i + 1);
            println!("{}{}", indent, category);
        }

        // Print the gloss with appropriate indentation
        let indent = "  ".repeat(current_category_path.len() + 1);
        println!("{}• {:?}", indent, gloss.gloss_lines);

        last_category_path = current_category_path;
    }
}

fn pretty_print(wn: &str, w: WordWithTaggedEntries) {
    // Get IPA pronunciation and hyphenation from word level
    let mut all_ipa: Vec<String> = Vec::new();

    if let Some(sound_str) = &w.sounds {
        all_ipa.push(sound_str.clone());
    }

    let hyph_str = &w.hyphenations.join("-");

    // Display word with pronunciation and hyphenation
    let ipa_str = if all_ipa.is_empty() {
        "".to_string()
    } else {
        all_ipa.join(", ")
    };

    println!("{wn} - {} - {}", ipa_str, hyph_str);

    // Display entries based on tag
    // println!("{w:#?}");
    match w.tag {
        WordTag::Monolingual => {
            // All entries are monolingual
            for entry in &w.entries {
                for sense in &entry.senses {
                    display_glosses_with_categories(&sense.glosses, &sense.pos, "[MONO] ");
                }
            }
        }
        WordTag::English => {
            // All entries are English
            for entry in &w.entries {
                for sense in &entry.senses {
                    display_glosses_with_categories(&sense.glosses, &sense.pos, "[ENG] ");
                }
            }
        }
        WordTag::Both => {
            // Exactly 2 entries: first is monolingual, second is English
            assert_eq!(w.entries.len(), 2, "Both tag must have exactly 2 entries");

            // First entry is monolingual
            for sense in &w.entries[0].senses {
                display_glosses_with_categories(&sense.glosses, &sense.pos, "[MONO] ");
            }

            // Second entry is English
            for sense in &w.entries[1].senses {
                display_glosses_with_categories(&sense.glosses, &sense.pos, "[ENG] ");
            }
        }
    }
}

fn main() {
    let s = Instant::now();
    //let f = File::open("en-dictionary.dict").unwrap();
    let f = File::open("es-multi-dictionary.dict").unwrap();
    let bf = BufReader::new(f);
    //let f = File::open("en-multi-dictionary.dict").unwrap();
    let mut d = DictionaryReader::open(bf).unwrap();
    println!("read {:?}", s.elapsed());
    let s = Instant::now();
    //let lookup = "perro";
    let lookup = "arroz";
    let r = d.lookup(lookup).unwrap();
    println!("looked 1st up {:?}", s.elapsed());
    if let Some(w) = r {
        // println!("{w:#?}");
        pretty_print(lookup, w);
    } else {
        println!("not found: '{lookup}'")
    };
    let r = d.lookup(lookup).unwrap();
    println!("looked 1st up {:?}", s.elapsed());
    if let Some(w) = r {
        // println!("{w:#?}");
        pretty_print(lookup, w);
    } else {
        println!("not found: '{lookup}'")
    };
}
