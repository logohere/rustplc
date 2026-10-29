use open_rust_plc_compat_core::image::ProcessImage;
use open_rust_plc_compat_core::journal::{
    JournalBufferType, JournalEntry, JournalQueue, QueueError,
};

#[test]
fn requires_initialization_and_preserves_order() {
    let queue = JournalQueue::new();
    let entry = JournalEntry::int(JournalBufferType::IntOutput, 2, 7).unwrap();
    assert_eq!(queue.enqueue(entry), Err(QueueError::Uninitialized));
    queue.init().unwrap();
    queue.enqueue(entry).unwrap();
    queue
        .enqueue(JournalEntry::int(JournalBufferType::IntOutput, 2, 9).unwrap())
        .unwrap();
    let mut image = ProcessImage::default();
    let stats = queue.apply_and_clear(&mut image).unwrap();
    assert_eq!(stats.applied, 2);
    assert_eq!(image.read(entry).unwrap(), 9);
    assert_eq!(queue.pending_count().unwrap(), 0);
    assert_eq!(queue.sequence().unwrap(), 0);
}

#[test]
fn reports_forced_and_invalid_entries() {
    let queue = JournalQueue::new();
    queue.init().unwrap();
    let forced = JournalEntry::byte(JournalBufferType::ByteOutput, 1, 3).unwrap();
    let invalid = JournalEntry::byte(JournalBufferType::ByteOutput, 1024, 4).unwrap();
    let mut image = ProcessImage::default();
    image.force_set(forced).unwrap();
    queue.enqueue(forced).unwrap();
    queue.enqueue(invalid).unwrap();
    let stats = queue.apply_and_clear(&mut image).unwrap();
    assert_eq!(stats.forced, 1);
    assert_eq!(stats.invalid, 1);
}
