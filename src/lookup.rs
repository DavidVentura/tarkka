use std::{fs::File, time::Instant};

use tarkka::AggregatedWord;

mod reader;

fn pretty_print(wn: &str, w: AggregatedWord) {
    let ipa = match w.ipa_sound {
        Some(ref v) if v.len() > 0 => v.first().unwrap(),
        _ => "",
    };

    let hyphenation = match w.hyphenation {
        Some(ref h) if h.len() > 0 => h.join("-"),
        _ => "".into(),
    };
    println!("{wn} - {} - {}", ipa, hyphenation);
    let mut last_category = "".to_string();
    for pg in w.pos_glosses {
        println!("{}:", pg.pos);
        for gloss in pg.glosses {
            if let Some(category) = &gloss.category {
                if category != &last_category {
                    println!("  {}", category);
                    last_category = category.to_string();
                }
                for glos in &gloss.glosses {
                    println!("   - {}", glos);
                }
            } else {
                for glos in &gloss.glosses {
                    println!(" - {}", glos);
                }
            }
        }
    }
}

fn main() {
    let s = Instant::now();
    let f = File::open("en-dictionary.dict").unwrap();
    let mut d = reader::DictionaryReader::open(f).unwrap();
    println!("read {:?}", s.elapsed());
    let s = Instant::now();
    let lookup = "deer";
    let r = d.lookup(lookup).unwrap();
    println!("looked 1st up {:?}", s.elapsed());
    if let Some(w) = r {
        pretty_print(lookup, w);
    } else {
        println!("not found: '{lookup}'")
    };
    let lookup = "potato";
    let r = d.lookup(lookup).unwrap();
    println!("looked 1st up {:?}", s.elapsed());
    if let Some(w) = r {
        pretty_print(lookup, w);
    } else {
        println!("not found: '{lookup}'")
    };
}
