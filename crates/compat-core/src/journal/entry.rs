use super::{JournalBufferType, BOOL_BITS, BUFFER_SIZE, NON_BOOL_BIT};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JournalError {
    InvalidType(u8),
    WrongType,
    InvalidBit(u8),
    InvalidNonBoolBit(u8),
    OutOfBounds(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JournalEntry {
    pub sequence: u32,
    pub buffer_type: JournalBufferType,
    pub bit_index: u8,
    pub index: u16,
    pub value: u64,
}

impl JournalEntry {
    fn new(buffer_type: JournalBufferType, index: u16, bit_index: u8, value: u64) -> Self {
        Self {
            sequence: 0,
            buffer_type,
            bit_index,
            index,
            value,
        }
    }

    pub fn bool(
        buffer_type: JournalBufferType,
        index: u16,
        bit: u8,
        value: bool,
    ) -> Result<Self, JournalError> {
        if !buffer_type.is_bool() {
            return Err(JournalError::WrongType);
        }
        if bit >= BOOL_BITS {
            return Err(JournalError::InvalidBit(bit));
        }
        Ok(Self::new(buffer_type, index, bit, u64::from(value)))
    }

    pub fn byte(
        buffer_type: JournalBufferType,
        index: u16,
        value: u8,
    ) -> Result<Self, JournalError> {
        if !matches!(
            buffer_type,
            JournalBufferType::ByteInput | JournalBufferType::ByteOutput
        ) {
            return Err(JournalError::WrongType);
        }
        Ok(Self::new(
            buffer_type,
            index,
            NON_BOOL_BIT,
            u64::from(value),
        ))
    }

    pub fn int(
        buffer_type: JournalBufferType,
        index: u16,
        value: u16,
    ) -> Result<Self, JournalError> {
        if !matches!(
            buffer_type,
            JournalBufferType::IntInput
                | JournalBufferType::IntOutput
                | JournalBufferType::IntMemory
        ) {
            return Err(JournalError::WrongType);
        }
        Ok(Self::new(
            buffer_type,
            index,
            NON_BOOL_BIT,
            u64::from(value),
        ))
    }

    pub fn dint(
        buffer_type: JournalBufferType,
        index: u16,
        value: u32,
    ) -> Result<Self, JournalError> {
        if !matches!(
            buffer_type,
            JournalBufferType::DintInput
                | JournalBufferType::DintOutput
                | JournalBufferType::DintMemory
        ) {
            return Err(JournalError::WrongType);
        }
        Ok(Self::new(
            buffer_type,
            index,
            NON_BOOL_BIT,
            u64::from(value),
        ))
    }

    pub fn lint(
        buffer_type: JournalBufferType,
        index: u16,
        value: u64,
    ) -> Result<Self, JournalError> {
        if !matches!(
            buffer_type,
            JournalBufferType::LintInput
                | JournalBufferType::LintOutput
                | JournalBufferType::LintMemory
        ) {
            return Err(JournalError::WrongType);
        }
        Ok(Self::new(buffer_type, index, NON_BOOL_BIT, value))
    }

    pub const fn with_sequence(mut self, sequence: u32) -> Self {
        self.sequence = sequence;
        self
    }

    pub fn validate_for_apply(self) -> Result<(), JournalError> {
        if usize::from(self.index) >= BUFFER_SIZE {
            return Err(JournalError::OutOfBounds(self.index));
        }
        if self.buffer_type.is_bool() {
            if self.bit_index >= BOOL_BITS {
                return Err(JournalError::InvalidBit(self.bit_index));
            }
        } else if self.bit_index != NON_BOOL_BIT {
            return Err(JournalError::InvalidNonBoolBit(self.bit_index));
        }
        Ok(())
    }

    pub const fn normalized_value(self) -> u64 {
        match self.buffer_type {
            JournalBufferType::BoolInput
            | JournalBufferType::BoolOutput
            | JournalBufferType::BoolMemory => self.value & 1,
            JournalBufferType::ByteInput | JournalBufferType::ByteOutput => self.value & 0xff,
            JournalBufferType::IntInput
            | JournalBufferType::IntOutput
            | JournalBufferType::IntMemory => self.value & 0xffff,
            JournalBufferType::DintInput
            | JournalBufferType::DintOutput
            | JournalBufferType::DintMemory => self.value & 0xffff_ffff,
            JournalBufferType::LintInput
            | JournalBufferType::LintOutput
            | JournalBufferType::LintMemory => self.value,
        }
    }
}
