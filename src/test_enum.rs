use crate::ser::CompactSerialize;
use std::io::Cursor;

#[derive(CompactSerialize, Clone, Copy)]
#[repr(u8)]
enum TestEnum {
    First = 1,
    Second = 2,
    Third = 42,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enum_serialization() {
        let mut buf = Vec::new();
        let mut cursor = Cursor::new(&mut buf);

        TestEnum::First.serialize(&mut cursor).unwrap();
        assert_eq!(buf, vec![1]);

        buf.clear();
        cursor.set_position(0);

        TestEnum::Second.serialize(&mut cursor).unwrap();
        assert_eq!(buf, vec![2]);

        buf.clear();
        cursor.set_position(0);

        TestEnum::Third.serialize(&mut cursor).unwrap();
        assert_eq!(buf, vec![42]);
    }
}