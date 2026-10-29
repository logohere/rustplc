//! OpenPLC Runtime v4 process-image journal entry contract.

pub const BUFFER_SIZE: usize = 1024;
pub const JOURNAL_MAX_ENTRIES: usize = 4096;
pub const BOOL_BITS: u8 = 8;
pub const NON_BOOL_BIT: u8 = 0xff;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JournalBufferType {
    BoolInput = 0,
    BoolOutput = 1,
    BoolMemory = 2,
    ByteInput = 3,
    ByteOutput = 4,
    IntInput = 5,
    IntOutput = 6,
    IntMemory = 7,
    DintInput = 8,
    DintOutput = 9,
    DintMemory = 10,
    LintInput = 11,
    LintOutput = 12,
    LintMemory = 13,
}

impl JournalBufferType {
    pub const fn is_bool(self) -> bool {
        matches!(self, Self::BoolInput | Self::BoolOutput | Self::BoolMemory)
    }
}

mod entry;
pub use entry::{JournalEntry, JournalError};
mod queue;
pub use queue::{ApplyStats, JournalQueue, QueueError};

impl TryFrom<u8> for JournalBufferType {
    type Error = JournalError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        let kind = match value {
            0 => Self::BoolInput,
            1 => Self::BoolOutput,
            2 => Self::BoolMemory,
            3 => Self::ByteInput,
            4 => Self::ByteOutput,
            5 => Self::IntInput,
            6 => Self::IntOutput,
            7 => Self::IntMemory,
            8 => Self::DintInput,
            9 => Self::DintOutput,
            10 => Self::DintMemory,
            11 => Self::LintInput,
            12 => Self::LintOutput,
            13 => Self::LintMemory,
            value => return Err(JournalError::InvalidType(value)),
        };
        Ok(kind)
    }
}
