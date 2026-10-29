use std::ffi::c_void;

use crate::abi::{LocatedArea, LocatedSize, LocatedVar};
use crate::journal::{JournalBufferType, JournalEntry};

use super::{globals, table, LocatedImage, LocatedImageError};

fn word(area: LocatedArea, index: u16, storage: &mut u16) -> LocatedVar {
    LocatedVar {
        area: area as u8,
        size: LocatedSize::Word as u8,
        byte_index: index,
        bit_index: 0,
        reserved: [0; 3],
        pointer: (storage as *mut u16).cast::<c_void>(),
    }
}

#[test]
fn dirty_output_is_committed_on_the_next_drain() {
    let mut member = 0;
    let located = LocatedImage::build(
        vec![word(LocatedArea::Output, 4, &mut member)],
        Vec::new(),
        None,
    )
    .unwrap();
    let address = JournalEntry::int(JournalBufferType::IntOutput, 4, 55).unwrap();
    located.write(address).unwrap();
    located.copy_in_ranges([(0, 1)]).unwrap();
    assert_eq!(member, 55);
    member = 77;
    assert_eq!(member, 77);
    located.copy_out_ranges([(0, 1)]).unwrap();
    assert_eq!(located.read(address).unwrap(), 77);
}

#[test]
fn input_member_changes_are_not_committed() {
    let mut member = 0;
    let located = LocatedImage::build(
        vec![word(LocatedArea::Input, 2, &mut member)],
        Vec::new(),
        None,
    )
    .unwrap();
    let address = JournalEntry::int(JournalBufferType::IntInput, 2, 10).unwrap();
    located.write(address).unwrap();
    located.copy_in_ranges([(0, 1)]).unwrap();
    member = 99;
    assert_eq!(member, 99);
    located.copy_out_ranges([(0, 1)]).unwrap();
    assert_eq!(located.read(address).unwrap(), 10);
}

#[test]
fn invalid_descriptor_fails_before_scanning() {
    let mut member = 0;
    let mut var = word(LocatedArea::Input, 0, &mut member);
    var.area = 9;
    assert!(matches!(
        LocatedImage::build(vec![var], Vec::new(), None),
        Err(LocatedImageError::Abi(_))
    ));
}

#[test]
fn empty_descriptor_table_does_not_dereference_null() {
    let copied = unsafe { table::copy(std::ptr::null(), 0) }.unwrap();
    assert!(copied.is_empty());
}

#[test]
fn misaligned_descriptor_table_fails_closed() {
    let raw = std::ptr::dangling::<u8>().cast::<c_void>();
    assert_eq!(
        unsafe { table::copy(raw, 1) }.unwrap_err(),
        LocatedImageError::MisalignedTable
    );
}

#[test]
fn misaligned_member_pointer_fails_closed() {
    let mut member = 0;
    let mut var = word(LocatedArea::Input, 0, &mut member);
    var.pointer = std::ptr::dangling::<u8>().cast::<c_void>().cast_mut();
    assert!(matches!(
        LocatedImage::build(vec![var], Vec::new(), None),
        Err(LocatedImageError::MisalignedMember)
    ));
}

#[test]
fn joins_configuration_globals_by_storage_identity() {
    let mut local = 0;
    let mut global = 0;
    let vars = vec![
        word(LocatedArea::Input, 0, &mut local),
        word(LocatedArea::Input, 1, &mut global),
    ];
    let pointers = [vars[1].pointer];
    assert_eq!(
        unsafe { globals::indices(&vars, Some((pointers.as_ptr(), 1))) }.unwrap(),
        vec![1]
    );
    assert_eq!(
        unsafe { globals::indices(&vars, Some((std::ptr::null(), 1))) }.unwrap_err(),
        LocatedImageError::NullGlobalsTable
    );
}
