use crate::AggregatedWord;
use std::fs::File;
use std::io::Read;
use std::time::Instant;

pub struct DictionaryReader {
    level1_data: Vec<u8>,
    level2_data: Vec<u8>,
    compressed_json: Vec<u8>,
    decompressed_json: Option<String>,
}

impl DictionaryReader {
    pub fn open(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let mut file = File::open(path)?;

        let mut magic = [0u8; 4];
        file.read_exact(&mut magic)?;
        if &magic != b"DICT" {
            return Err("Invalid dictionary file format".into());
        }

        let mut size_buf = [0u8; 4];

        file.read_exact(&mut size_buf)?;
        let level1_size = u32::from_le_bytes(size_buf) as usize;

        file.read_exact(&mut size_buf)?;
        let level2_size = u32::from_le_bytes(size_buf) as usize;

        file.read_exact(&mut size_buf)?;
        let json_size = u32::from_le_bytes(size_buf) as usize;

        let mut level1_data = vec![0u8; level1_size];
        file.read_exact(&mut level1_data)?;

        let mut level2_data = vec![0u8; level2_size];
        file.read_exact(&mut level2_data)?;

        let mut compressed_json = vec![0u8; json_size];
        file.read_exact(&mut compressed_json)?;

        Ok(DictionaryReader {
            level1_data,
            level2_data,
            compressed_json,
            decompressed_json: None,
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

        let json_offset = self.find_in_level2_group(level2_offset, group_size, word)?;
        if json_offset.is_none() {
            return Ok(None);
        }

        self.get_word_from_json(json_offset.unwrap(), word)
    }

    fn find_level2_offset(
        &self,
        first_char: &str,
    ) -> Result<Option<(u32, u32)>, Box<dyn std::error::Error>> {
        let target_bytes = first_char.as_bytes();
        let mut pos = 0;

        while pos < self.level1_data.len() {
            // Read UTF-8 character
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
        &self,
        group_offset: u32,
        group_size: u32,
        word: &str,
    ) -> Result<Option<u32>, Box<dyn std::error::Error>> {
        let group_start = group_offset as usize;
        let group_end = group_start + group_size as usize;

        if group_start >= self.level2_data.len() || group_end > self.level2_data.len() {
            return Ok(None);
        }

        let compressed_group = &self.level2_data[group_start..group_end];
        let decompressed = zstd::decode_all(compressed_group)?;

        // Search in decompressed group data
        let mut pos = 0;
        while pos < decompressed.len() {
            if pos + 1 > decompressed.len() {
                break;
            }

            let word_len = decompressed[pos] as usize;
            pos += 1;

            if pos + word_len + 4 > decompressed.len() {
                break;
            }

            let entry_word = std::str::from_utf8(&decompressed[pos..pos + word_len])?;
            pos += word_len;

            let offset_bytes = &decompressed[pos..pos + 4];
            let json_offset = u32::from_le_bytes([
                offset_bytes[0],
                offset_bytes[1],
                offset_bytes[2],
                offset_bytes[3],
            ]);
            pos += 4;

            if entry_word == word {
                return Ok(Some(json_offset));
            }
        }

        Ok(None)
    }

    fn get_word_from_json(
        &mut self,
        offset: u32,
        word: &str,
    ) -> Result<Option<AggregatedWord>, Box<dyn std::error::Error>> {
        if self.decompressed_json.is_none() {
            let s = Instant::now();
            let decompressed = zstd::decode_all(self.compressed_json.as_slice())?;
            println!("decompressed = {:?}", s.elapsed());
            self.decompressed_json = Some(unsafe { String::from_utf8_unchecked(decompressed) });
            println!("stringed = {:?}", s.elapsed());
        }

        let json_data = self.decompressed_json.as_ref().unwrap();
        let offset = offset as usize;

        if offset >= json_data.len() {
            return Ok(None);
        }

        let line_end = json_data[offset..]
            .find('\n')
            .unwrap_or(json_data.len() - offset);
        let line = &json_data[offset..offset + line_end];

        let parsed: AggregatedWord = serde_json::from_str(line)?;

        if parsed.word == word {
            Ok(Some(parsed))
        } else {
            Ok(None)
        }
    }
}
