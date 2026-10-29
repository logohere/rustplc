use open_rust_plc_compat_core::image::ProcessImage;
use open_rust_plc_compat_core::journal::{JournalBufferType, JournalEntry, JournalError};

#[test]
fn applies_widths_and_boolean_bits() {
    let mut image = ProcessImage::default();
    let dint = JournalEntry::dint(JournalBufferType::DintOutput, 4, u32::MAX).unwrap();
    assert!(image.apply(dint).unwrap());
    assert_eq!(image.read(dint).unwrap(), u64::from(u32::MAX));
    let bit = JournalEntry::bool(JournalBufferType::BoolOutput, 4, 3, true).unwrap();
    image.apply(bit).unwrap();
    assert_eq!(image.read(bit).unwrap(), 1);
}

#[test]
fn forced_slot_rejects_until_cleared() {
    let mut image = ProcessImage::default();
    let entry = JournalEntry::int(JournalBufferType::IntOutput, 1, 55).unwrap();
    image.force_set(entry).unwrap();
    let update = JournalEntry::int(JournalBufferType::IntOutput, 1, 99).unwrap();
    assert!(!image.apply(update).unwrap());
    assert_eq!(image.read(update).unwrap(), 55);
    image
        .force_clear(JournalBufferType::IntOutput, 1, 0xff)
        .unwrap();
    assert!(image.apply(update).unwrap());
    assert_eq!(image.read(update).unwrap(), 99);
}

#[test]
fn rejects_invalid_descriptor_before_indexing() {
    let mut image = ProcessImage::default();
    let bad = JournalEntry::byte(JournalBufferType::ByteInput, 1024, 1).unwrap();
    assert_eq!(image.apply(bad), Err(JournalError::OutOfBounds(1024)));
}
