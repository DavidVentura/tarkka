use crate::ser::VarUint;
use crate::{HEADER_SIZE, WordWithTaggedEntries};
use std::io::Seek;
use std::io::{Read, SeekFrom};

struct OffsetFile<R: Read + Seek> {
    reader: R,
    base_offset: u64,
}

impl<R: Read + Seek> OffsetFile<R> {
    fn new(mut r: R, base_offset: u64) -> std::io::Result<Self> {
        r.seek(SeekFrom::Start(base_offset))?;
        Ok(Self {
            reader: r,
            base_offset,
        })
    }
}

impl<R: Read + Seek> Read for OffsetFile<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.reader.read(buf)
    }
}

impl<R: Read + Seek> Seek for OffsetFile<R> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let adjusted_pos = match pos {
            SeekFrom::Start(offset) => SeekFrom::Start(self.base_offset + offset),
            SeekFrom::Current(offset) => SeekFrom::Current(offset),
            SeekFrom::End(offset) => SeekFrom::End(offset),
        };
        let result = self.reader.seek(adjusted_pos)?;
        Ok(result - self.base_offset)
    }
}

pub struct DictionaryReader<'a, R: Read + Seek> {
    level1_data: Vec<u8>,
    level2_size: u32,
    binary_data_off: u32,
    decoder: zeekstd::Decoder<'a, OffsetFile<R>>,
}

impl<'a, R: Read + Seek> DictionaryReader<'a, R> {
    pub fn open(mut r: R) -> Result<Self, Box<dyn std::error::Error>> {
        let mut magic = [0u8; 4];
        r.read_exact(&mut magic)?;
        if &magic != b"DICT" {
            return Err("Invalid dictionary file format".into());
        }

        let mut size_buf = [0u8; 4];

        r.read_exact(&mut size_buf)?;
        let level1_size = u32::from_le_bytes(size_buf);

        r.read_exact(&mut size_buf)?;
        let level2_size = u32::from_le_bytes(size_buf);

        r.read_exact(&mut size_buf)?;
        let _binary_data_size = u32::from_le_bytes(size_buf);

        let mut level1_data = vec![0u8; level1_size as usize];
        r.read_exact(&mut level1_data)?;

        let level2_off = level1_size + HEADER_SIZE as u32;
        let offset_file = OffsetFile::new(r, level2_off as u64)?;
        let decoder = zeekstd::Decoder::new(offset_file).unwrap();

        let binary_data_off = level2_size;
        Ok(DictionaryReader {
            level1_data,
            level2_size,
            binary_data_off,
            decoder,
        })
    }

    pub fn lookup(
        &mut self,
        word: &str,
    ) -> Result<Option<WordWithTaggedEntries>, Box<dyn std::error::Error>> {
        let word_bytes = word.as_bytes();
        let l1_group = match word_bytes.len() {
            0 => return Err("Empty word".into()),
            1 => [0, 0, word_bytes[0]],
            2 => [0, word_bytes[0], word_bytes[1]],
            _ => [word_bytes[0], word_bytes[1], word_bytes[2]],
        };

        let (group_offset, group_size, binary_base_offset) =
            match self.find_level2_group_info(&l1_group)? {
                Some((offset, size, binary_offset)) => (offset, size, binary_offset),
                None => return Ok(None),
            };

        let result = self.find_in_level2_group(group_offset, group_size, word)?;
        if result.is_none() {
            return Ok(None);
        }

        let (relative_binary_offset, binary_size) = result.unwrap();
        let absolute_binary_offset =
            binary_base_offset + relative_binary_offset + self.binary_data_off;
        match self.get_word_from_binary_data(absolute_binary_offset, binary_size, word) {
            Ok(w) => Ok(Some(w)),
            Err(e) => Err(e),
        }
    }

    fn find_level2_group_info(
        &self,
        l1_group: &[u8; 3],
    ) -> Result<Option<(u32, u32, u32)>, Box<dyn std::error::Error>> {
        let mut pos = 0;
        let mut group_offset = 0u32;

        while pos + 11 <= self.level1_data.len() {
            let key = [
                self.level1_data[pos],
                self.level1_data[pos + 1],
                self.level1_data[pos + 2],
            ];

            let size_bytes = &self.level1_data[pos + 3..pos + 7];
            let size =
                u32::from_le_bytes([size_bytes[0], size_bytes[1], size_bytes[2], size_bytes[3]]);

            if &key == l1_group {
                let binary_offset_bytes = &self.level1_data[pos + 7..pos + 11];
                let binary_offset = u32::from_le_bytes([
                    binary_offset_bytes[0],
                    binary_offset_bytes[1],
                    binary_offset_bytes[2],
                    binary_offset_bytes[3],
                ]);
                return Ok(Some((group_offset, size, binary_offset)));
            }

            group_offset += size;
            pos += 11; // 3 bytes key + 4 bytes size + 4 bytes binary offset
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

        let wanted_word_b = word.as_bytes();
        let mut pos = 0;
        let mut current_word: Vec<u8> = Vec::new();
        let mut binary_offset = 0u32;

        while pos + 4 <= decompressed.len() {
            let shared_len = decompressed[pos] as usize;
            pos += 1;

            let suffix_len = decompressed[pos] as usize;
            pos += 1;

            if pos + suffix_len + 2 > decompressed.len() {
                panic!("malformed data, expected suffix but got EOF");
            }

            assert!(suffix_len > 0);
            let suffix_b = &decompressed[pos..pos + suffix_len];
            pos += suffix_len;

            // TODO remove alloc with mem:swap
            let mut word_buf = Vec::new();
            if shared_len > 0 {
                assert!(current_word.len() > 0);
                word_buf.extend_from_slice(&current_word[0..shared_len]);
            }
            word_buf.extend_from_slice(suffix_b);

            // Read binary data size VarUint
            let maybe_size_bytes = &decompressed[pos..pos + 2];
            pos += 1;
            let first_byte = maybe_size_bytes[0];
            let second_byte = maybe_size_bytes[1];
            let binary_size = if (first_byte & 0x80) == 0x80 {
                pos += 1;
                ((first_byte & 0x7F) as u16) | ((second_byte as u16) << 7)
            } else {
                first_byte as u16
            };

            if word_buf == wanted_word_b {
                return Ok(Some((binary_offset, binary_size)));
            }
            current_word = word_buf;

            binary_offset += binary_size as u32;
        }

        Ok(None)
    }

    fn get_word_from_binary_data(
        &mut self,
        offset: u32,
        size: u16,
        word: &str,
    ) -> Result<WordWithTaggedEntries, Box<dyn std::error::Error>> {
        self.decoder.set_offset(offset as u64)?;
        self.decoder.set_offset_limit(offset as u64 + size as u64)?;

        let mut decompressed = Vec::new();
        std::io::copy(&mut self.decoder, &mut decompressed)?;
        let parsed = WordWithTaggedEntries::named_deserialize(
            &mut decompressed.as_slice(),
            word.to_string(),
        )
        .map_err(|e| -> Box<dyn std::error::Error> { Box::from(e) })?;

        Ok(parsed)
    }
}

/*
fn reuse_vec<T, U>(mut v: Vec<T>) -> Vec<U> {
    const {
        assert!(size_of::<T>() == size_of::<U>());
        assert!(align_of::<T>() == align_of::<U>());
    }
    v.clear();
    v.into_iter().map(|_| unreachable!()).collect()
}
*/
