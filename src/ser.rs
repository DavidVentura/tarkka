use std::io::{self, Write};

#[derive(Debug)]
pub enum SerializeError {
    IoError(io::Error),
}

impl From<io::Error> for SerializeError {
    fn from(err: io::Error) -> Self {
        SerializeError::IoError(err)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum MaxLen {
    OneByte,
    TwoBytes,
    TwoBytesVar,
}

pub trait CompactSerialize {
    fn serialize<W: Write>(&self, out: &mut W) -> Result<usize, SerializeError>;
}

pub trait CompactSerializeWithMaxLen {
    fn serialize<W: Write>(&self, out: &mut W, max_len: MaxLen) -> Result<usize, SerializeError>;
}

impl CompactSerialize for u8 {
    fn serialize<W: Write>(&self, out: &mut W) -> Result<usize, SerializeError> {
        out.write_all(&[*self])?;
        Ok(1)
    }
}

impl CompactSerialize for u16 {
    fn serialize<W: Write>(&self, out: &mut W) -> Result<usize, SerializeError> {
        out.write_all(&self.to_le_bytes())?;
        Ok(2)
    }
}

impl CompactSerialize for u32 {
    fn serialize<W: Write>(&self, out: &mut W) -> Result<usize, SerializeError> {
        out.write_all(&self.to_le_bytes())?;
        Ok(4)
    }
}

impl<T: CompactSerialize> CompactSerializeWithMaxLen for Vec<T> {
    fn serialize<W: Write>(&self, out: &mut W, max_len: MaxLen) -> Result<usize, SerializeError> {
        self.as_slice().serialize(out, max_len)
    }
}

impl<T: CompactSerialize> CompactSerializeWithMaxLen for &[T] {
    fn serialize<W: Write>(&self, out: &mut W, max_len: MaxLen) -> Result<usize, SerializeError> {
        let mut size = 0;
        match max_len {
            MaxLen::OneByte => {
                assert!(self.len() <= u8::MAX as usize);
                size += 1;
                out.write_all(&[self.len() as u8])?;
            }
            MaxLen::TwoBytes => {
                assert!(self.len() <= u16::MAX as usize);
                size += 2;
                out.write_all(&(self.len() as u16).to_le_bytes())?;
            }
            MaxLen::TwoBytesVar => {
                let vu: VarUint = self.len().into();
                size += vu.serialize(out)?;
            }
        }
        for item in self.iter() {
            size += item.serialize(out)?;
        }
        Ok(size)
    }
}

impl CompactSerialize for String {
    // TODO: this forces varint str
    fn serialize<W: Write>(&self, out: &mut W) -> Result<usize, SerializeError> {
        let vu: VarUint = self.len().into();
        vu.serialize(out)?;
        let b = self.as_bytes();
        out.write_all(b)?;
        Ok(vu.serialized_len() + b.len())
    }
}

impl CompactSerialize for Option<String> {
    fn serialize<W: Write>(&self, out: &mut W) -> Result<usize, SerializeError> {
        if let Some(s) = self {
            return s.serialize(out);
        }
        out.write_all(&[0])?;
        Ok(1)
    }
}

pub struct VarUint(u16);

impl VarUint {
    fn serialized_len(&self) -> usize {
        if self.0 <= 127 {
            return 1;
        }
        2
    }
}
impl From<usize> for VarUint {
    fn from(value: usize) -> Self {
        assert!(value < (u16::MAX / 2) as usize);
        Self(value as u16)
    }
}
impl From<u16> for VarUint {
    fn from(value: u16) -> Self {
        assert!(value < (u16::MAX / 2));
        Self(value)
    }
}
impl From<u8> for VarUint {
    fn from(value: u8) -> Self {
        Self(value as u16)
    }
}

impl CompactSerialize for VarUint {
    fn serialize<W: Write>(&self, out: &mut W) -> Result<usize, SerializeError> {
        // if first bit is 1; then we have two bytes; meaning that
        // if val >127 we need 2 bytes
        if self.0 > 127 {
            let fb = (self.0 & 0x7F) as u8;
            let sb = (self.0 >> 7) as u8;
            out.write_all(&[fb | 0x80, sb])?;
        } else {
            out.write_all(&[self.0 as u8])?;
        };
        Ok(self.serialized_len())
    }
}
pub use tarkka_derive::CompactSerialize;

impl<T> CompactSerialize for &T
where
    T: CompactSerialize,
{
    fn serialize<W: Write>(&self, out: &mut W) -> Result<usize, SerializeError> {
        (**self).serialize(out)
    }
}
