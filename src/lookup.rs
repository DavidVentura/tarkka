use std::time::Instant;

mod reader;

fn main() {
    let s = Instant::now();
    let mut d = reader::DictionaryReader::open("dictionary.dict").unwrap();
    println!("read {:?}", s.elapsed());
    let s = Instant::now();
    let r = d.lookup("Alemania").unwrap();
    println!("looked 1st up {:?}", s.elapsed());
    println!("word = {r:?}");
    let r = d.lookup("hola").unwrap();
    println!("looked 2nd up {:?}", s.elapsed());
    println!("word = {r:?}");
}
