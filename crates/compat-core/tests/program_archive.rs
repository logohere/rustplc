#[path = "program_archive/support.rs"]
mod support;

use std::fs;
use std::path::PathBuf;

use open_rust_plc_compat_core::program_archive::{
    stage_archive, ArchiveError, ArchiveViolation, MAX_ARCHIVE_BYTES, MAX_ENTRIES, MAX_FILE_BYTES,
};
use support::{assert_violation, relative_files, repeated_bytes, Fixture};
use zip::CompressionMethod;

#[test]
fn extracts_single_root_and_normalizes_editor_paths() {
    let fixture = Fixture::new("editor");
    fixture.write(
        &[
            ("project/generated.hpp", b"header".as_slice()),
            ("project/generated.cpp", b"source".as_slice()),
            ("__MACOSX/project/._generated.cpp", b"junk".as_slice()),
            ("project/.DS_Store", b"junk".as_slice()),
        ],
        CompressionMethod::Stored,
    );
    let report = stage_archive(&fixture.archive, &fixture.staging).unwrap();
    assert_eq!(report.file_count, 2);
    assert_eq!(
        relative_files(&fixture.staging),
        ["generated.cpp", "generated.hpp"]
            .map(PathBuf::from)
            .to_vec()
    );
}

#[test]
fn preserves_raw_root_detection_before_windows_separator_normalization() {
    let fixture = Fixture::new("windows-roots");
    fixture.write(
        &[
            ("project\\generated.cpp", b"source".as_slice()),
            ("project\\generated.hpp", b"header".as_slice()),
        ],
        CompressionMethod::Stored,
    );
    stage_archive(&fixture.archive, &fixture.staging).unwrap();
    assert_eq!(
        relative_files(&fixture.staging),
        ["project/generated.cpp", "project/generated.hpp"]
            .map(PathBuf::from)
            .to_vec()
    );
}

#[test]
fn preserves_multiple_roots() {
    let fixture = Fixture::new("roots");
    fixture.write(
        &[
            ("generated.cpp", b"source".as_slice()),
            ("conf/modbus.json", b"{}".as_slice()),
        ],
        CompressionMethod::Bzip2,
    );
    stage_archive(&fixture.archive, &fixture.staging).unwrap();
    assert_eq!(
        relative_files(&fixture.staging),
        ["conf/modbus.json", "generated.cpp"]
            .map(PathBuf::from)
            .to_vec()
    );
}

#[test]
fn rejects_unsafe_and_blocked_paths_before_staging() {
    for (index, name) in [
        "../escape.cpp",
        "/absolute.cpp",
        "C:/drive.cpp",
        "project\\..\\escape.cpp",
        "project/unsafe.JS",
    ]
    .into_iter()
    .enumerate()
    {
        let fixture = Fixture::new(&format!("path-{index}"));
        fixture.write(&[(name, b"x")], CompressionMethod::Stored);
        let result = stage_archive(&fixture.archive, &fixture.staging);
        assert!(result.is_err());
        assert!(!fixture.staging.exists());
    }
}

#[test]
fn accepts_double_dot_inside_a_path_component() {
    let fixture = Fixture::new("double-dot-name");
    fixture.write(
        &[("project/foo..bar.cpp", b"source")],
        CompressionMethod::Stored,
    );
    stage_archive(&fixture.archive, &fixture.staging).unwrap();
    assert_eq!(
        relative_files(&fixture.staging),
        vec![PathBuf::from("foo..bar.cpp")]
    );
}

#[test]
fn rejects_duplicate_normalized_paths() {
    let fixture = Fixture::new("duplicate");
    fixture.write(
        &[("project/a.cpp", b"a"), ("project\\a.cpp", b"b")],
        CompressionMethod::Stored,
    );
    assert_violation(
        stage_archive(&fixture.archive, &fixture.staging),
        ArchiveViolation::DuplicatePath,
    );
}

#[test]
fn rejects_archive_symlinks() {
    let fixture = Fixture::new("symlink");
    fixture.write_symlink();
    assert_violation(
        stage_archive(&fixture.archive, &fixture.staging),
        ArchiveViolation::NonRegularEntry,
    );
}

#[test]
fn rejects_encrypted_and_unsupported_entries() {
    let encrypted = Fixture::new("encrypted");
    encrypted.write(&[("a.cpp", b"source")], CompressionMethod::Stored);
    encrypted.patch_headers(6, 8, 1);
    assert_violation(
        stage_archive(&encrypted.archive, &encrypted.staging),
        ArchiveViolation::Encrypted,
    );

    let unsupported = Fixture::new("unsupported");
    unsupported.write(&[("a.cpp", b"source")], CompressionMethod::Stored);
    unsupported.patch_headers(8, 10, 98);
    assert_violation(
        stage_archive(&unsupported.archive, &unsupported.staging),
        ArchiveViolation::UnsupportedCompression,
    );
}

#[test]
fn bounds_archive_entry_count() {
    let fixture = Fixture::new("entries");
    fixture.write_empty_files(MAX_ENTRIES + 1);
    assert_violation(
        stage_archive(&fixture.archive, &fixture.staging),
        ArchiveViolation::TooManyEntries,
    );
}

#[test]
fn bounds_total_uncompressed_size() {
    let fixture = Fixture::new("total");
    let contents = repeated_bytes(1024 * 1024);
    fixture.write_copies(6, &contents);
    fixture.patch_uncompressed_size(9 * 1024 * 1024);
    assert!(fs::metadata(&fixture.archive).unwrap().len() < MAX_ARCHIVE_BYTES);
    assert_violation(
        stage_archive(&fixture.archive, &fixture.staging),
        ArchiveViolation::TotalTooLarge,
    );
    assert!(!fixture.staging.exists());
}

#[test]
fn enforces_archive_file_and_ratio_limits() {
    let raw = Fixture::new("raw-limit");
    fs::write(&raw.archive, vec![0_u8; MAX_ARCHIVE_BYTES as usize + 1]).unwrap();
    assert_violation(
        stage_archive(&raw.archive, &raw.staging),
        ArchiveViolation::ArchiveTooLarge,
    );

    let large = Fixture::new("file-limit");
    let contents = repeated_bytes(MAX_FILE_BYTES as usize + 1);
    large.write(&[("large.cpp", &contents)], CompressionMethod::Deflated);
    assert_violation(
        stage_archive(&large.archive, &large.staging),
        ArchiveViolation::FileTooLarge,
    );

    let ratio = Fixture::new("ratio");
    let zeros = vec![0_u8; 2 * 1024 * 1024];
    ratio.write(&[("bomb.cpp", &zeros)], CompressionMethod::Deflated);
    assert_violation(
        stage_archive(&ratio.archive, &ratio.staging),
        ArchiveViolation::CompressionRatio,
    );
}

#[test]
fn corrupt_payload_removes_staging() {
    let fixture = Fixture::new("crc");
    let payload = b"unique-payload-for-crc";
    fixture.write(&[("project/a.cpp", payload)], CompressionMethod::Stored);
    let mut bytes = fs::read(&fixture.archive).unwrap();
    let offset = bytes
        .windows(payload.len())
        .position(|item| item == payload)
        .unwrap();
    bytes[offset] ^= 0xff;
    fs::write(&fixture.archive, bytes).unwrap();
    assert!(stage_archive(&fixture.archive, &fixture.staging).is_err());
    assert!(!fixture.staging.exists());
}

#[test]
fn existing_destination_is_unchanged() {
    let fixture = Fixture::new("existing");
    fixture.write(&[("a.cpp", b"source")], CompressionMethod::Stored);
    fs::create_dir(&fixture.staging).unwrap();
    fs::write(fixture.staging.join("prior"), b"keep").unwrap();
    assert!(matches!(
        stage_archive(&fixture.archive, &fixture.staging),
        Err(ArchiveError::DestinationExists)
    ));
    assert_eq!(fs::read(fixture.staging.join("prior")).unwrap(), b"keep");
}

#[test]
fn rejects_different_local_and_central_header_names_before_staging() {
    let fixture = Fixture::new("header-names");
    fixture.write(&[("safe.cpp", b"source")], CompressionMethod::Stored);
    let mut bytes = fs::read(&fixture.archive).unwrap();
    let central = bytes
        .windows(4)
        .position(|window| window == b"PK\x01\x02")
        .unwrap();
    assert_eq!(&bytes[central + 46..central + 54], b"safe.cpp");
    bytes[central + 46..central + 54].copy_from_slice(b"evil.cpp");
    fs::write(&fixture.archive, bytes).unwrap();

    assert!(stage_archive(&fixture.archive, &fixture.staging).is_err());
    assert!(!fixture.staging.exists());
}
