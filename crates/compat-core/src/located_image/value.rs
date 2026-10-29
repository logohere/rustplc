use crate::abi::{AbiError, LocatedArea, LocatedSize, LocatedVar};
use crate::journal::{JournalBufferType, JournalEntry, JournalError};

pub(super) fn entry(var: LocatedVar, value: u64) -> Result<JournalEntry, ValueError> {
    let area = var.area_kind()?;
    let size = var.size_kind()?;
    if !var.pointer.is_null() && !(var.pointer as usize).is_multiple_of(member_alignment(size)) {
        return Err(ValueError::MisalignedMember);
    }
    let kind = buffer_type(area, size);
    let entry = match size {
        LocatedSize::Bit => JournalEntry::bool(kind, var.byte_index, var.bit_index, value != 0),
        LocatedSize::Byte => JournalEntry::byte(kind, var.byte_index, value as u8),
        LocatedSize::Word => JournalEntry::int(kind, var.byte_index, value as u16),
        LocatedSize::DWord => JournalEntry::dint(kind, var.byte_index, value as u32),
        LocatedSize::LWord => JournalEntry::lint(kind, var.byte_index, value),
    }?;
    entry.validate_for_apply()?;
    Ok(entry)
}

pub(super) unsafe fn read_member(var: LocatedVar) -> u64 {
    if var.pointer.is_null() {
        return 0;
    }
    match var.size_kind().expect("validated located size") {
        LocatedSize::Bit | LocatedSize::Byte => u64::from(*var.pointer.cast::<u8>()),
        LocatedSize::Word => u64::from(*var.pointer.cast::<u16>()),
        LocatedSize::DWord => u64::from(*var.pointer.cast::<u32>()),
        LocatedSize::LWord => *var.pointer.cast::<u64>(),
    }
}

pub(super) unsafe fn write_member(var: LocatedVar, value: u64) {
    if var.pointer.is_null() {
        return;
    }
    match var.size_kind().expect("validated located size") {
        LocatedSize::Bit => *var.pointer.cast::<u8>() = (value & 1) as u8,
        LocatedSize::Byte => *var.pointer.cast::<u8>() = value as u8,
        LocatedSize::Word => *var.pointer.cast::<u16>() = value as u16,
        LocatedSize::DWord => *var.pointer.cast::<u32>() = value as u32,
        LocatedSize::LWord => *var.pointer.cast::<u64>() = value,
    }
}

fn buffer_type(area: LocatedArea, size: LocatedSize) -> JournalBufferType {
    use JournalBufferType as Buffer;
    match (area, size) {
        (LocatedArea::Input, LocatedSize::Bit) => Buffer::BoolInput,
        (LocatedArea::Output, LocatedSize::Bit) => Buffer::BoolOutput,
        (LocatedArea::Memory, LocatedSize::Bit) => Buffer::BoolMemory,
        (LocatedArea::Input, LocatedSize::Byte) => Buffer::ByteInput,
        (LocatedArea::Output | LocatedArea::Memory, LocatedSize::Byte) => Buffer::ByteOutput,
        (LocatedArea::Input, LocatedSize::Word) => Buffer::IntInput,
        (LocatedArea::Output, LocatedSize::Word) => Buffer::IntOutput,
        (LocatedArea::Memory, LocatedSize::Word) => Buffer::IntMemory,
        (LocatedArea::Input, LocatedSize::DWord) => Buffer::DintInput,
        (LocatedArea::Output, LocatedSize::DWord) => Buffer::DintOutput,
        (LocatedArea::Memory, LocatedSize::DWord) => Buffer::DintMemory,
        (LocatedArea::Input, LocatedSize::LWord) => Buffer::LintInput,
        (LocatedArea::Output, LocatedSize::LWord) => Buffer::LintOutput,
        (LocatedArea::Memory, LocatedSize::LWord) => Buffer::LintMemory,
    }
}

fn member_alignment(size: LocatedSize) -> usize {
    match size {
        LocatedSize::Bit | LocatedSize::Byte => std::mem::align_of::<u8>(),
        LocatedSize::Word => std::mem::align_of::<u16>(),
        LocatedSize::DWord => std::mem::align_of::<u32>(),
        LocatedSize::LWord => std::mem::align_of::<u64>(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ValueError {
    Abi(AbiError),
    Journal(JournalError),
    MisalignedMember,
}

impl From<AbiError> for ValueError {
    fn from(error: AbiError) -> Self {
        Self::Abi(error)
    }
}

impl From<JournalError> for ValueError {
    fn from(error: JournalError) -> Self {
        Self::Journal(error)
    }
}
