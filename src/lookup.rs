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
    let mut last_category_path: Vec<String> = vec![];
    for pg in w.pos_glosses {
        println!("{}:", pg.pos);
        for gloss in pg.glosses {
            // Find how much of the category path has changed
            let mut common_len = 0;
            while common_len < last_category_path.len()
                && common_len < gloss.category_path.len()
                && last_category_path[common_len] == gloss.category_path[common_len]
            {
                common_len += 1;
            }

            // Print any new category levels
            for (i, category) in gloss.category_path.iter().enumerate().skip(common_len) {
                let indent = " ".repeat((i + 1) * 2);
                println!("{}{}", indent, category);
            }

            // Print the gloss with appropriate indentation
            let indent = " ".repeat((gloss.category_path.len() + 1) * 2);
            println!("{}- {}", indent, gloss.gloss);

            last_category_path = gloss.category_path.clone();
        }
    }
}

fn main() {
    let s = Instant::now();
    let f = File::open("en-dictionary.dict").unwrap();
    let mut d = reader::DictionaryReader::open(f).unwrap();
    println!("read {:?}", s.elapsed());
    let s = Instant::now();
    let lookup = "Denmark";
    let r = d.lookup(lookup).unwrap();
    println!("looked 1st up {:?}", s.elapsed());
    if let Some(w) = r {
        pretty_print(lookup, w);
    } else {
        println!("not found: '{lookup}'")
    };
}
