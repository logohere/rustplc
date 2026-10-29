use crate::journal::{
    JournalBufferType, JournalEntry, JournalError, BOOL_BITS, BUFFER_SIZE, NON_BOOL_BIT,
};

const BOOL_BANKS: usize = 3;
const BYTE_BANKS: usize = 2;
const SCALAR_BANKS: usize = 3;
const ALL_SCALAR_BANKS: usize = 11;

#[derive(Clone, Debug)]
pub struct ProcessImage {
    bools: [[[bool; BOOL_BITS as usize]; BUFFER_SIZE]; BOOL_BANKS],
    bytes: [[u8; BUFFER_SIZE]; BYTE_BANKS],
    ints: [[u16; BUFFER_SIZE]; SCALAR_BANKS],
    dints: [[u32; BUFFER_SIZE]; SCALAR_BANKS],
    lints: [[u64; BUFFER_SIZE]; SCALAR_BANKS],
    forced_bools: [[u8; BUFFER_SIZE]; BOOL_BANKS],
    forced_scalars: [[u8; BUFFER_SIZE]; ALL_SCALAR_BANKS],
}

impl Default for ProcessImage {
    fn default() -> Self {
        Self {
            bools: [[[false; BOOL_BITS as usize]; BUFFER_SIZE]; BOOL_BANKS],
            bytes: [[0; BUFFER_SIZE]; BYTE_BANKS],
            ints: [[0; BUFFER_SIZE]; SCALAR_BANKS],
            dints: [[0; BUFFER_SIZE]; SCALAR_BANKS],
            lints: [[0; BUFFER_SIZE]; SCALAR_BANKS],
            forced_bools: [[0; BUFFER_SIZE]; BOOL_BANKS],
            forced_scalars: [[0; BUFFER_SIZE]; ALL_SCALAR_BANKS],
        }
    }
}

impl ProcessImage {
    pub fn apply(&mut self, entry: JournalEntry) -> Result<bool, JournalError> {
        entry.validate_for_apply()?;
        if self.is_forced(entry) {
            return Ok(false);
        }
        self.write_raw(entry);
        Ok(true)
    }

    pub fn force_set(&mut self, entry: JournalEntry) -> Result<(), JournalError> {
        entry.validate_for_apply()?;
        self.mark_forced(entry, true);
        self.write_raw(entry);
        Ok(())
    }

    pub fn force_clear(
        &mut self,
        buffer_type: JournalBufferType,
        index: u16,
        bit_index: u8,
    ) -> Result<(), JournalError> {
        let entry = JournalEntry {
            sequence: 0,
            buffer_type,
            bit_index,
            index,
            value: 0,
        };
        entry.validate_for_apply()?;
        self.mark_forced(entry, false);
        Ok(())
    }

    pub fn read(&self, entry: JournalEntry) -> Result<u64, JournalError> {
        entry.validate_for_apply()?;
        let i = usize::from(entry.index);
        let value = match entry.buffer_type {
            JournalBufferType::BoolInput
            | JournalBufferType::BoolOutput
            | JournalBufferType::BoolMemory => {
                u64::from(self.bools[bool_bank(entry.buffer_type)][i][entry.bit_index as usize])
            }
            JournalBufferType::ByteInput | JournalBufferType::ByteOutput => {
                u64::from(self.bytes[byte_bank(entry.buffer_type)][i])
            }
            JournalBufferType::IntInput
            | JournalBufferType::IntOutput
            | JournalBufferType::IntMemory => {
                u64::from(self.ints[scalar_bank(entry.buffer_type)][i])
            }
            JournalBufferType::DintInput
            | JournalBufferType::DintOutput
            | JournalBufferType::DintMemory => {
                u64::from(self.dints[scalar_bank(entry.buffer_type)][i])
            }
            JournalBufferType::LintInput
            | JournalBufferType::LintOutput
            | JournalBufferType::LintMemory => self.lints[scalar_bank(entry.buffer_type)][i],
        };
        Ok(value)
    }

    fn is_forced(&self, entry: JournalEntry) -> bool {
        let i = usize::from(entry.index);
        if entry.buffer_type.is_bool() {
            self.forced_bools[bool_bank(entry.buffer_type)][i] & (1 << entry.bit_index) != 0
        } else {
            self.forced_scalars[scalar_force_bank(entry.buffer_type)][i] != 0
        }
    }

    fn mark_forced(&mut self, entry: JournalEntry, forced: bool) {
        let i = usize::from(entry.index);
        if entry.buffer_type.is_bool() {
            let mask = 1 << entry.bit_index;
            if forced {
                self.forced_bools[bool_bank(entry.buffer_type)][i] |= mask;
            } else {
                self.forced_bools[bool_bank(entry.buffer_type)][i] &= !mask;
            }
        } else {
            self.forced_scalars[scalar_force_bank(entry.buffer_type)][i] = u8::from(forced);
        }
    }

    fn write_raw(&mut self, entry: JournalEntry) {
        let i = usize::from(entry.index);
        let value = entry.normalized_value();
        match entry.buffer_type {
            JournalBufferType::BoolInput
            | JournalBufferType::BoolOutput
            | JournalBufferType::BoolMemory => {
                self.bools[bool_bank(entry.buffer_type)][i][entry.bit_index as usize] = value != 0
            }
            JournalBufferType::ByteInput | JournalBufferType::ByteOutput => {
                self.bytes[byte_bank(entry.buffer_type)][i] = value as u8
            }
            JournalBufferType::IntInput
            | JournalBufferType::IntOutput
            | JournalBufferType::IntMemory => {
                self.ints[scalar_bank(entry.buffer_type)][i] = value as u16
            }
            JournalBufferType::DintInput
            | JournalBufferType::DintOutput
            | JournalBufferType::DintMemory => {
                self.dints[scalar_bank(entry.buffer_type)][i] = value as u32
            }
            JournalBufferType::LintInput
            | JournalBufferType::LintOutput
            | JournalBufferType::LintMemory => {
                self.lints[scalar_bank(entry.buffer_type)][i] = value
            }
        }
    }
}

const fn bool_bank(kind: JournalBufferType) -> usize {
    match kind {
        JournalBufferType::BoolInput => 0,
        JournalBufferType::BoolOutput => 1,
        JournalBufferType::BoolMemory => 2,
        _ => unreachable!(),
    }
}

const fn byte_bank(kind: JournalBufferType) -> usize {
    match kind {
        JournalBufferType::ByteInput => 0,
        JournalBufferType::ByteOutput => 1,
        _ => unreachable!(),
    }
}

const fn scalar_bank(kind: JournalBufferType) -> usize {
    match kind {
        JournalBufferType::IntInput
        | JournalBufferType::DintInput
        | JournalBufferType::LintInput => 0,
        JournalBufferType::IntOutput
        | JournalBufferType::DintOutput
        | JournalBufferType::LintOutput => 1,
        _ => 2,
    }
}

const fn scalar_force_bank(kind: JournalBufferType) -> usize {
    match kind {
        JournalBufferType::ByteInput => 0,
        JournalBufferType::ByteOutput => 1,
        JournalBufferType::IntInput => 2,
        JournalBufferType::IntOutput => 3,
        JournalBufferType::IntMemory => 4,
        JournalBufferType::DintInput => 5,
        JournalBufferType::DintOutput => 6,
        JournalBufferType::DintMemory => 7,
        JournalBufferType::LintInput => 8,
        JournalBufferType::LintOutput => 9,
        JournalBufferType::LintMemory => 10,
        _ => unreachable!(),
    }
}

const _: () = assert!(NON_BOOL_BIT == 0xff);
