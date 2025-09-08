use std::error::Error as StdError;
use std::fmt;
use std::io::{self, Read};

#[derive(Debug)]
pub enum DeserializeError {
    IoError(io::Error),
    InvalidData(&'static str),
}

impl fmt::Display for DeserializeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeserializeError::IoError(err) => write!(f, "IO error: {}", err),
            DeserializeError::InvalidData(msg) => write!(f, "Invalid data: {}", msg),
        }
    }
}

impl StdError for DeserializeError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            DeserializeError::IoError(err) => Some(err),
            DeserializeError::InvalidData(_) => None,
        }
    }
}

impl From<io::Error> for DeserializeError {
    fn from(err: io::Error) -> Self {
        DeserializeError::IoError(err)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum MaxLen {
    OneByte,
    TwoBytes,
    TwoBytesVar,
}

pub trait CompactDeserialize: Sized {
    fn deserialize<R: Read>(input: &mut R) -> Result<Self, DeserializeError>;
}

pub trait CompactDeserializeWithMaxLen: Sized {
    fn deserialize<R: Read>(input: &mut R, max_len: MaxLen) -> Result<Self, DeserializeError>;
}

impl CompactDeserialize for u8 {
    fn deserialize<R: Read>(input: &mut R) -> Result<Self, DeserializeError> {
        let mut buf = [0u8; 1];
        input.read_exact(&mut buf)?;
        Ok(buf[0])
    }
}

impl CompactDeserialize for u16 {
    fn deserialize<R: Read>(input: &mut R) -> Result<Self, DeserializeError> {
        let mut buf = [0u8; 2];
        input.read_exact(&mut buf)?;
        Ok(u16::from_le_bytes(buf))
    }
}

impl CompactDeserialize for u32 {
    fn deserialize<R: Read>(input: &mut R) -> Result<Self, DeserializeError> {
        let mut buf = [0u8; 4];
        input.read_exact(&mut buf)?;
        Ok(u32::from_le_bytes(buf))
    }
}

impl<T: CompactDeserialize> CompactDeserializeWithMaxLen for Vec<T> {
    fn deserialize<R: Read>(input: &mut R, max_len: MaxLen) -> Result<Self, DeserializeError> {
        let len = match max_len {
            MaxLen::OneByte => {
                let len_u8 = u8::deserialize(input)?;
                len_u8 as usize
            }
            MaxLen::TwoBytes => {
                let len_u16 = u16::deserialize(input)?;
                len_u16 as usize
            }
            MaxLen::TwoBytesVar => {
                let vu = VarUint::deserialize(input)?;
                vu.0 as usize
            }
        };

        let mut vec = Vec::with_capacity(len);
        for _ in 0..len {
            vec.push(T::deserialize(input)?);
        }
        Ok(vec)
    }
}

impl CompactDeserialize for String {
    fn deserialize<R: Read>(input: &mut R) -> Result<Self, DeserializeError> {
        let vu = VarUint::deserialize(input)?;
        let len = vu.0 as usize;

        let mut buf = vec![0u8; len];
        input.read_exact(&mut buf)?;

        String::from_utf8(buf).map_err(|_| DeserializeError::InvalidData("Invalid UTF-8 string"))
    }
}

impl CompactDeserialize for Option<String> {
    fn deserialize<R: Read>(input: &mut R) -> Result<Self, DeserializeError> {
        let vu = VarUint::deserialize(input)?;
        if vu.0 == 0 {
            Ok(None)
        } else {
            let len = vu.0 as usize;
            let mut buf = vec![0u8; len];
            input.read_exact(&mut buf)?;

            let string = String::from_utf8(buf)
                .map_err(|_| DeserializeError::InvalidData("Invalid UTF-8 string"))?;
            Ok(Some(string))
        }
    }
}

pub struct VarUint(pub u16);

impl CompactDeserialize for VarUint {
    fn deserialize<R: Read>(input: &mut R) -> Result<Self, DeserializeError> {
        let mut buf = [0u8; 1];
        input.read_exact(&mut buf)?;
        let first_byte = buf[0];

        if first_byte & 0x80 == 0 {
            // Single byte
            Ok(VarUint(first_byte as u16))
        } else {
            // Two bytes
            input.read_exact(&mut buf)?;
            let second_byte = buf[0];
            let value = ((first_byte & 0x7F) as u16) | ((second_byte as u16) << 7);
            Ok(VarUint(value))
        }
    }
}

pub use tarkka_derive::CompactDeserialize;
