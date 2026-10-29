use open_rust_plc_compat_core::journal::{
    JournalBufferType, JournalEntry, JournalError, BUFFER_SIZE, NON_BOOL_BIT,
};

#[test]
fn preserves_type_values_and_non_bool_sentinel() {
    assert_eq!(JournalBufferType::BoolInput as u8, 0);
    assert_eq!(JournalBufferType::LintMemory as u8, 13);
    let entry = JournalEntry::dint(JournalBufferType::DintOutput, 2, u32::MAX)
        .unwrap()
        .with_sequence(7);
    assert_eq!(entry.bit_index, NON_BOOL_BIT);
    assert_eq!(entry.sequence, 7);
    assert_eq!(entry.normalized_value(), u64::from(u32::MAX));
}

#[test]
fn rejects_wrong_type_and_invalid_bits() {
    assert_eq!(
        JournalEntry::bool(JournalBufferType::ByteInput, 0, 0, true),
        Err(JournalError::WrongType)
    );
    assert_eq!(
        JournalEntry::bool(JournalBufferType::BoolOutput, 0, 8, true),
        Err(JournalError::InvalidBit(8))
    );
    assert_eq!(
        JournalBufferType::try_from(14),
        Err(JournalError::InvalidType(14))
    );
}

#[test]
fn keeps_enqueue_and_apply_bounds_distinct() {
    let entry = JournalEntry::byte(JournalBufferType::ByteInput, BUFFER_SIZE as u16, 1).unwrap();
    assert_eq!(
        entry.validate_for_apply(),
        Err(JournalError::OutOfBounds(1024))
    );
    let bad = JournalEntry {
        index: 0,
        bit_index: 2,
        ..entry
    };
    assert_eq!(
        bad.validate_for_apply(),
        Err(JournalError::InvalidNonBoolBit(2))
    );
}
