use std::collections::HashSet;
use std::fs::{File, create_dir_all};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tarkka::kaikki::WordEntry;
use threadpool::ThreadPool;

// Supported languages from Language.kt
const SUPPORTED_LANGUAGES: &[&str] = &[
    "sq", "ar", "az", "bn", "bg", "ca", "zh", "hr", "cs", "da", "nl", "en", "et", "fi", "fr", "de",
    "el", "gu", "he", "hi", "hu", "id", "it", "ja", "kn", "ko", "lv", "lt", "ms", "ml", "fa", "pl",
    "pt", "ro", "ru", "sk", "sl", "es", "sv", "ta", "te", "tr", "uk", "is",
];

fn download_file(
    url: &str,
    output_path: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if Path::new(output_path).exists() {
        println!("File already exists: {}", output_path);
        return Ok(());
    }

    println!("Downloading: {} -> {}", url, output_path);
    let response = ureq::get(url).call();

    match response {
        Ok(resp) => {
            let mut file = File::create(output_path)?;
            std::io::copy(&mut resp.into_reader(), &mut file)?;
            println!("Downloaded successfully: {}", output_path);
            Ok(())
        }
        Err(ureq::Error::Status(404, _)) => {
            println!("File not found (404): {}", url);
            Err("404 Not Found".into())
        }
        Err(e) => Err(format!("HTTP error: {}", e).into()),
    }
}

fn split_file(
    input_file: &str,
    output_dir: &str,
    filter_lang: Option<&str>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let start = Instant::now();
    let mut stats = std::collections::HashMap::new();
    let mut total_lines = 0;
    let mut skipped_lines = 0;
    let mut writers = std::collections::HashMap::new();

    // Create output directory
    create_dir_all(output_dir)?;

    // Create supported languages set
    let supported_langs: HashSet<&str> = SUPPORTED_LANGUAGES.iter().cloned().collect();

    // Open and decompress input file
    let file = File::open(input_file)?;
    let decoder = flate2::read::GzDecoder::new(file);
    let reader = BufReader::new(decoder);

    for line in reader.lines() {
        let line = line?;
        total_lines += 1;

        if line.is_empty() {
            skipped_lines += 1;
            continue;
        }

        // Parse JSON to get language code
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

        // Skip phrases with spaces
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
            let file = File::create(&output_file)?;
            let writer = BufWriter::new(file);
            writers.insert(lang_code.clone(), writer);
            println!("Created output file: {}", output_file);
        }

        // Write line to the appropriate file
        let writer = writers.get_mut(&lang_code).unwrap();
        writeln!(writer, "{}", line)?;

        // Update stats
        *stats.entry(lang_code).or_insert(0) += 1;
    }

    // Flush all writers
    for (_, writer) in writers.iter_mut() {
        writer.flush()?;
    }

    println!("Split completed in {:?}", start.elapsed());
    println!("Total lines processed: {}", total_lines);
    println!("Lines skipped: {}", skipped_lines);
    println!("Languages found: {}", stats.len());

    // Print stats sorted by language code
    let mut sorted_stats: Vec<_> = stats.into_iter().collect();
    sorted_stats.sort_by(|a, b| a.0.cmp(&b.0));

    for (lang, count) in sorted_stats {
        println!("  {}: {} entries", lang, count);
    }

    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <output_directory>", args[0]);
        std::process::exit(1);
    }

    let output_dir = &args[1];
    println!("Starting download and split process to: {}", output_dir);

    // Create directory structure
    let downloads_dir = format!("{}/downloads", output_dir);
    let english_dir = format!("{}/english", output_dir);
    let monolingual_dir = format!("{}/monolingual", output_dir);

    if let Err(e) = create_dir_all(&downloads_dir) {
        eprintln!("Error creating downloads directory: {}", e);
        std::process::exit(1);
    }
    if let Err(e) = create_dir_all(&english_dir) {
        eprintln!("Error creating english directory: {}", e);
        std::process::exit(1);
    }
    if let Err(e) = create_dir_all(&monolingual_dir) {
        eprintln!("Error creating monolingual directory: {}", e);
        std::process::exit(1);
    }

    // Create thread pool and shared data structures
    let pool = ThreadPool::new(4);
    let downloaded_files = Arc::new(Mutex::new(Vec::new()));
    let failed_downloads = Arc::new(Mutex::new(Vec::new()));

    // Prepare all download tasks
    let mut download_tasks = Vec::new();

    // English file
    download_tasks.push((
        "en".to_string(),
        "https://kaikki.org/dictionary/raw-wiktextract-data.jsonl.gz".to_string(),
        format!("{}/en-extract.jsonl.gz", downloads_dir),
    ));

    // Other language files
    for &lang in SUPPORTED_LANGUAGES {
        if lang == "en" {
            continue; // Already added above
        }

        let url = format!(
            "https://kaikki.org/dictionary/downloads/{}/{}-extract.jsonl.gz",
            lang, lang
        );
        let path = format!("{}/{}-extract.jsonl.gz", downloads_dir, lang);

        download_tasks.push((lang.to_string(), url, path));
    }

    // Execute downloads in thread pool
    for (lang, url, path) in download_tasks {
        let downloaded_files = Arc::clone(&downloaded_files);
        let failed_downloads = Arc::clone(&failed_downloads);

        pool.execute(move || match download_file(&url, &path) {
            Ok(()) => {
                downloaded_files.lock().unwrap().push((lang.clone(), path));
            }
            Err(e) => {
                if e.to_string().contains("404") {
                    if lang == "en" {
                        println!("English file not available (404), skipping...");
                    } else {
                        println!(
                            "Language file not available for '{}' (404), skipping...",
                            lang
                        );
                    }
                } else {
                    if lang == "en" {
                        eprintln!("Error downloading English file: {}", e);
                    } else {
                        eprintln!("Error downloading file for '{}': {}", lang, e);
                    }
                }
                failed_downloads.lock().unwrap().push(lang);
            }
        });
    }

    // Wait for all downloads to complete
    pool.join();

    let downloaded_files = Arc::try_unwrap(downloaded_files)
        .unwrap()
        .into_inner()
        .unwrap();
    let failed_downloads = Arc::try_unwrap(failed_downloads)
        .unwrap()
        .into_inner()
        .unwrap();

    println!("\nDownloads completed!");
    println!("Successfully downloaded: {} files", downloaded_files.len());
    println!("Failed/unavailable: {} files", failed_downloads.len());

    // Process downloaded files with thread pool
    let processing_pool = ThreadPool::new(4);
    let processing_results = Arc::new(Mutex::new(Vec::new()));

    for (lang, file_path) in downloaded_files {
        let english_dir = english_dir.clone();
        let monolingual_dir = monolingual_dir.clone();
        let processing_results = Arc::clone(&processing_results);

        processing_pool.execute(move || {
            println!("Processing: {} ({})", lang, file_path);

            let result = if lang == "en" {
                // English: split all languages to english directory
                match split_file(&file_path, &english_dir, None) {
                    Ok(()) => {
                        println!("Successfully processed English file");
                        Ok(())
                    }
                    Err(e) => {
                        eprintln!("Error processing English file: {}", e);
                        Err(e)
                    }
                }
            } else {
                // Other languages: filter to specific language, output to monolingual directory
                match split_file(&file_path, &monolingual_dir, Some(&lang)) {
                    Ok(()) => {
                        println!("Successfully processed {} file", lang);
                        Ok(())
                    }
                    Err(e) => {
                        eprintln!("Error processing {} file: {}", lang, e);
                        Err(e)
                    }
                }
            };

            processing_results.lock().unwrap().push((lang, result));
        });
    }

    // Wait for all processing to complete
    processing_pool.join();

    let processing_results = Arc::try_unwrap(processing_results)
        .unwrap()
        .into_inner()
        .unwrap();
    let successful_processing = processing_results
        .iter()
        .filter(|(_, result)| result.is_ok())
        .count();
    let failed_processing = processing_results.len() - successful_processing;

    println!("\nAll processing completed!");
    println!("Successfully processed: {} files", successful_processing);
    println!("Failed processing: {} files", failed_processing);
}
