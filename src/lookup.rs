use std::time::Instant;

use tarkka::AggregatedWord;

mod reader;

fn pretty_print(wn: &str, w: AggregatedWord) {
    let ipa = match w.ipa_sound {
        Some(ref v) if v.len() > 0 => v.first().unwrap(),
        _ => "",
    };

    let hyphenation = match w.hyphenation {
        Some(ref h) if h.len() > 0 => h.first().unwrap(),
        _ => "",
    };
    println!("{wn} - {} - {}", ipa, hyphenation);
    for pg in w.pos_glosses {
        println!("{}:", pg.pos);
        for glos in pg.glosses {
            println!(" - {}", glos);
        }
    }
}

fn main() {
    let s = Instant::now();
    let mut d = reader::DictionaryReader::open("dictionary.dict").unwrap();
    println!("read {:?}", s.elapsed());
    let s = Instant::now();
    let r = d.lookup("Austria").unwrap();
    println!("looked 1st up {:?}", s.elapsed());
    if let Some(w) = r {
        pretty_print("cane", w);
    };
}
