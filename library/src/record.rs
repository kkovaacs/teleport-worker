use std::convert::TryFrom;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RecordType {
    Stdout = 0,
    Stderr = 1,
}

impl TryFrom<u8> for RecordType {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            x if x == Self::Stdout as u8 => Ok(Self::Stdout),
            x if x == Self::Stderr as u8 => Ok(Self::Stderr),
            _ => Err(()),
        }
    }
}

/// Represents a log record.
///
/// This is basically the type of the record (stdout or stderr), plus a chunk of binary data that
/// was read from the process.
#[derive(Debug, PartialEq)]
pub struct Record {
    pub record_type: RecordType,
    pub data: Vec<u8>,
}

pub(crate) const HEADER_LENGTH: usize = 1 + std::mem::size_of::<u64>();

impl Record {
    pub fn len(&self) -> usize {
        HEADER_LENGTH + self.data.len()
    }
}
