use crate::ser::CompactSerialize;

#[derive(CompactSerialize)]
struct TestStruct {
    id: u32,
    #[max_len_cat(OneByte)]
    name: String,
    #[max_len_cat(TwoBytes)]
    tags: Vec<String>,
    #[max_len_cat(OneByte)]
    tags2: Vec<String>,
    nested: NestedStruct,
}

#[derive(CompactSerialize)]
struct NestedStruct {
    value: u8,
    code: u16,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_primitive_serialization() {
        let mut buf = Vec::new();
        let mut cursor = Cursor::new(&mut buf);

        let value: u32 = 0x12345678;
        value.serialize(&mut cursor).unwrap();

        assert_eq!(buf, vec![0x78, 0x56, 0x34, 0x12]); // little endian
    }

    #[test]
    fn test_struct_serialization() {
        let mut buf = Vec::new();
        let mut cursor = Cursor::new(&mut buf);

        let long_str = String::from_utf8(vec![b'X'; 128]).unwrap(); // needs 2 bytes len
        let test_data = TestStruct {
            id: 0x1234,
            name: "test".to_string(),
            tags: vec!["aaa".to_string(), "bbb".to_string()],
            tags2: vec!["".to_string(); 255],
            nested: NestedStruct {
                value: 42,
                code: 0x5678,
            },
        };

        test_data.serialize(&mut cursor).unwrap();

        // Verify the serialized format
        println!("Serialized data: {:?}", buf);
        assert!(buf.len() > 0);
        assert_eq!(buf[0], 0x34);
        assert_eq!(buf[1], 0x12);
        assert_eq!(buf[2], 0x00);
        assert_eq!(buf[3], 0x00);
        // str len
        assert_eq!(buf[4], 4);
        assert_eq!(buf[5..9], ['t' as u8, 'e' as u8, 's' as u8, 't' as u8]);
        // vec len (u16)
        assert_eq!(buf[9], 0x2);
        assert_eq!(buf[10], 0x0);
        // first str, len=3 (u8)
        assert_eq!(buf[11], 0x3);
        assert_eq!(buf[12..15], ['a' as u8, 'a' as u8, 'a' as u8]);
        // str is len 128 => 2 byte len
        assert_eq!(buf[15], 0x03);
        assert_eq!(buf[16..19], ['b' as u8, 'b' as u8, 'b' as u8]);
        // can have 1 u8 => 255 is allowed
        assert_eq!(buf[19], 0xFF);
    }
}
