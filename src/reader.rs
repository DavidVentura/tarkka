use std::io::{Read, SeekFrom};
use std::{fs::File, io::Seek};
use tarkka::{AggregatedWord, HEADER_SIZE};

struct OffsetFile {
    file: File,
    base_offset: u64,
}

impl OffsetFile {
    fn new(mut file: File, base_offset: u64) -> std::io::Result<Self> {
        file.seek(SeekFrom::Start(base_offset))?;
        Ok(Self { file, base_offset })
    }
}

impl Read for OffsetFile {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.file.read(buf)
    }
}

impl Seek for OffsetFile {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let adjusted_pos = match pos {
            SeekFrom::Start(offset) => SeekFrom::Start(self.base_offset + offset),
            SeekFrom::Current(offset) => SeekFrom::Current(offset),
            SeekFrom::End(offset) => SeekFrom::End(offset),
        };
        let result = self.file.seek(adjusted_pos)?;
        Ok(result - self.base_offset)
    }
}

pub struct DictionaryReader<'a> {
    level1_data: Vec<u8>,
    level2_size: u32,
    json_off: u32,
    decoder: zeekstd::Decoder<'a, OffsetFile>,
}

impl<'a> DictionaryReader<'a> {
    pub fn open(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let mut file = File::open(path)?;

        let mut magic = [0u8; 4];
        file.read_exact(&mut magic)?;
        if &magic != b"DICT" {
            return Err("Invalid dictionary file format".into());
        }

        let mut size_buf = [0u8; 4];

        file.read_exact(&mut size_buf)?;
        let level1_size = u32::from_le_bytes(size_buf);

        file.read_exact(&mut size_buf)?;
        let level2_size = u32::from_le_bytes(size_buf);

        file.read_exact(&mut size_buf)?;
        let _json_size = u32::from_le_bytes(size_buf);

        let mut level1_data = vec![0u8; level1_size as usize];
        file.read_exact(&mut level1_data)?;

        let level2_off = level1_size + HEADER_SIZE as u32;
        let offset_file = OffsetFile::new(file, level2_off as u64)?;
        let decoder = zeekstd::Decoder::new(offset_file).unwrap();

        let json_off = level2_size;
        Ok(DictionaryReader {
            level1_data,
            level2_size,
            json_off,
            decoder,
        })
    }

    pub fn lookup(
        &mut self,
        word: &str,
    ) -> Result<Option<AggregatedWord>, Box<dyn std::error::Error>> {
        let first_char = word.chars().next().ok_or("Empty word")?;
        let first_char_bytes = first_char.to_string();

        let (level2_offset, group_size) = match self.find_level2_offset(&first_char_bytes)? {
            Some(result) => result,
            None => return Ok(None),
        };

        let result = self.find_in_level2_group(level2_offset, group_size, word)?;
        if result.is_none() {
            return Ok(None);
        }

        let (json_offset, json_size) = result.unwrap();
        match self.get_word_from_json(json_offset + self.json_off, json_size) {
            Ok(w) => Ok(Some(w)),
            Err(e) => Err(e),
        }
    }

    fn find_level2_offset(
        &self,
        first_char: &str,
    ) -> Result<Option<(u32, u32)>, Box<dyn std::error::Error>> {
        let target_bytes = first_char.as_bytes();
        let mut pos = 0;

        while pos < self.level1_data.len() {
            let char_start = pos;
            let first_byte = self.level1_data[pos];
            let char_len = if first_byte < 0x80 {
                1
            } else if first_byte < 0xE0 {
                2
            } else if first_byte < 0xF0 {
                3
            } else {
                4
            };

            if pos + char_len + 8 > self.level1_data.len() {
                break;
            }

            let char_bytes = &self.level1_data[char_start..char_start + char_len];

            if char_bytes == target_bytes {
                let offset_bytes =
                    &self.level1_data[char_start + char_len..char_start + char_len + 4];
                let offset = u32::from_le_bytes([
                    offset_bytes[0],
                    offset_bytes[1],
                    offset_bytes[2],
                    offset_bytes[3],
                ]);

                let size_bytes =
                    &self.level1_data[char_start + char_len + 4..char_start + char_len + 8];
                let size = u32::from_le_bytes([
                    size_bytes[0],
                    size_bytes[1],
                    size_bytes[2],
                    size_bytes[3],
                ]);

                return Ok(Some((offset, size)));
            }

            pos = char_start + char_len + 8;
        }

        Ok(None)
    }

    fn find_in_level2_group(
        &mut self,
        group_offset: u32,
        group_size: u32,
        word: &str,
    ) -> Result<Option<(u32, u16)>, Box<dyn std::error::Error>> {
        let group_start = group_offset;
        let group_end = group_start + group_size;

        if group_start >= self.level2_size || group_end > self.level2_size {
            return Ok(None);
        }

        self.decoder.set_offset(group_start as u64)?;
        self.decoder.set_offset_limit(group_end as u64)?;
        let mut decompressed = Vec::new();
        std::io::copy(&mut self.decoder, &mut decompressed)?;

        let mut pos = 0;
        let mut current_word = String::new();

        while pos < decompressed.len() {
            if pos + 2 > decompressed.len() {
                break;
            }

            // Read restart_flag(1bit) + shared_prefix_len(7bits)
            let prefix_byte = decompressed[pos];
            let is_restart = (prefix_byte & 0x80) != 0;
            let shared_len = (prefix_byte & 0x7F) as usize;
            pos += 1;

            let suffix_len = decompressed[pos] as usize;
            pos += 1;

            if pos + suffix_len + 6 > decompressed.len() {
                break;
            }

            let suffix =
                unsafe { std::str::from_utf8_unchecked(&decompressed[pos..pos + suffix_len]) };
            pos += suffix_len;

            if is_restart && shared_len == 0 {
                // For restart entries, reconstruct from the last restart word + current shared_len + suffix
                current_word = suffix.to_string();
            } else {
                // For non-restart entries, use shared_len from current restart word
                let truncate_pos = current_word
                    .char_indices()
                    .nth(shared_len)
                    .map(|(pos, _)| pos)
                    .unwrap_or(current_word.len());
                current_word.truncate(truncate_pos);
                current_word.push_str(suffix);
            }

            // Read JSON offset and size
            let offset_bytes = &decompressed[pos..pos + 4];
            let json_offset = u32::from_le_bytes([
                offset_bytes[0],
                offset_bytes[1],
                offset_bytes[2],
                offset_bytes[3],
            ]);
            pos += 4;

            let size_bytes = &decompressed[pos..pos + 2];
            let json_size = u16::from_le_bytes([size_bytes[0], size_bytes[1]]);
            pos += 2;

            if current_word == word {
                return Ok(Some((json_offset, json_size)));
            }
        }

        Ok(None)
    }

    fn get_word_from_json(
        &mut self,
        offset: u32,
        size: u16,
    ) -> Result<AggregatedWord, Box<dyn std::error::Error>> {
        let mut decompressed = Vec::new();
        self.decoder.set_offset(offset as u64)?;
        self.decoder.set_offset_limit(offset as u64 + size as u64)?;
        std::io::copy(&mut self.decoder, &mut decompressed)?;

        let json_entry = unsafe { String::from_utf8_unchecked(decompressed) };
        let parsed: AggregatedWord =
            serde_json::from_str(&json_entry).expect("failed to JSON decode");

        Ok(parsed)
    }
}
