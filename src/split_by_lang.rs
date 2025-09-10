use std::collections::{HashMap, HashSet};
use std::fs::{File, create_dir_all};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::time::Instant;
use tarkka::kaikki::WordEntry;

// Supported languages from Language.kt
const SUPPORTED_LANGUAGES: &[&str] = &[
    "sq", "ar", "az", "bn", "bg", "ca", "zh", "hr", "cs", "da", "nl", "en", "et", "fi", "fr", "de",
    "el", "gu", "he", "hi", "hu", "id", "it", "ja", "kn", "ko", "lv", "lt", "ms", "ml", "fa", "pl",
    "pt", "ro", "ru", "sk", "sl", "es", "sv", "ta", "te", "tr", "uk",
];

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Parse arguments - handle optional --filter
    let (input_file, output_dir, filter_lang) = if args.len() == 3 {
        // No filter: <binary> <input> <output>
        (&args[1], &args[2], None)
    } else if args.len() == 5 && args[1] == "--filter" {
        // With filter: <binary> --filter <lang> <input> <output>
        let lang = &args[2];
        let supported_langs: HashSet<&str> = SUPPORTED_LANGUAGES.iter().cloned().collect();
        if !supported_langs.contains(lang.as_str()) {
            eprintln!("Error: '{}' is not a supported language", lang);
            eprintln!("Supported languages: {}", SUPPORTED_LANGUAGES.join(", "));
            std::process::exit(1);
        }
        (&args[3], &args[4], Some(lang.as_str()))
    } else {
        eprintln!(
            "Usage: {} [--filter <language>] <input_file.jsonl> <output_directory>",
            args[0]
        );
        eprintln!("Example: {} input.jsonl output/", args[0]);
        eprintln!("Example: {} --filter es input.jsonl output/", args[0]);
        std::process::exit(1);
    };

    match filter_lang {
        Some(lang) => println!(
            "Splitting {} by language to {} (filtering for '{}')...",
            input_file, output_dir, lang
        ),
        None => println!("Splitting {} by language to {}...", input_file, output_dir),
    }

    let start = Instant::now();
    let mut writers: HashMap<String, BufWriter<File>> = HashMap::new();
    let mut stats: HashMap<String, usize> = HashMap::new();
    let mut total_lines = 0;
    let mut skipped_lines = 0;

    // Create output directory if it doesn't exist
    create_dir_all(output_dir).unwrap_or_else(|e| {
        eprintln!("Error creating output directory {}: {}", output_dir, e);
        std::process::exit(1);
    });

    // Create set for O(1) language lookup
    let supported_langs: HashSet<&str> = SUPPORTED_LANGUAGES.iter().cloned().collect();

    let file = File::open(input_file).unwrap_or_else(|e| {
        eprintln!("Error opening file {}: {}", input_file, e);
        std::process::exit(1);
    });

    let decoder = zstd::Decoder::new(file).unwrap_or_else(|e| {
        eprintln!("Error creating zstd decoder: {}", e);
        std::process::exit(1);
    });

    let reader = BufReader::new(decoder);

    for line in reader.lines() {
        let line = line.unwrap();
        total_lines += 1;

        if line.is_empty() {
            skipped_lines += 1;
            continue;
        }

        let word_entry: WordEntry = match serde_json::from_str(&line) {
            Ok(entry) => entry,
            Err(_) => {
                skipped_lines += 1;
                continue;
            }
        };

        let lang_code = match word_entry.lang_code {
            Some(code) => code,
            None => {
                skipped_lines += 1;
                continue;
            }
        };
        match word_entry.word {
            Some(w) => {
                if w.contains(" ") {
                    continue;
                }
            }
            None => continue,
        };

        // Only process supported languages
        if !supported_langs.contains(lang_code.as_str()) {
            skipped_lines += 1;
            continue;
        }

        // If filtering for specific language, skip others
        if let Some(filter) = filter_lang {
            if lang_code.as_str() != filter {
                skipped_lines += 1;
                continue;
            }
        }

        // Get or create writer for this language
        if !writers.contains_key(&lang_code) {
            let output_file = format!("{}/{}.jsonl", output_dir, lang_code);
            let file = File::create(&output_file).unwrap_or_else(|e| {
                eprintln!("Error creating file {}: {}", output_file, e);
                std::process::exit(1);
            });
            let writer = BufWriter::new(file);
            writers.insert(lang_code.clone(), writer);
            println!("Created output file: {}", output_file);
        }

        // Write line to the appropriate file
        let writer = writers.get_mut(&lang_code).unwrap();
        writeln!(writer, "{}", line).unwrap();

        // Update stats
        *stats.entry(lang_code).or_insert(0) += 1;

        if total_lines % 100_000 == 0 {
            println!("Processed {} lines...", total_lines);
        }
    }

    // Flush all writers
    for (_, writer) in writers.iter_mut() {
        writer.flush().unwrap();
    }

    println!("Completed in {:?}", start.elapsed());
    println!("Total lines processed: {}", total_lines);
    println!("Lines skipped: {}", skipped_lines);
    println!("Languages found: {}", stats.len());

    // Print stats sorted by language code
    let mut sorted_stats: Vec<_> = stats.into_iter().collect();
    sorted_stats.sort_by(|a, b| a.0.cmp(&b.0));

    for (lang, count) in sorted_stats {
        println!("  {}: {} entries", lang, count);
    }
}
