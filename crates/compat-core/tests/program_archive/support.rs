use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use open_rust_plc_compat_core::program_archive::{ArchiveError, ArchiveViolation};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

pub struct Fixture {
    pub root: PathBuf,
    pub archive: PathBuf,
    pub staging: PathBuf,
}

impl Fixture {
    pub fn new(label: &str) -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "rustplc-archive-{label}-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir(&root).unwrap();
        Self {
            archive: root.join("program.zip"),
            staging: root.join("staging"),
            root,
        }
    }

    pub fn write(&self, entries: &[(&str, &[u8])], method: CompressionMethod) {
        let file = File::create(&self.archive).unwrap();
        let mut writer = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(method);
        for (name, contents) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(contents).unwrap();
        }
        writer.finish().unwrap();
    }

    pub fn write_symlink(&self) {
        let file = File::create(&self.archive).unwrap();
        let mut writer = ZipWriter::new(file);
        writer
            .add_symlink("project/link", "target", SimpleFileOptions::default())
            .unwrap();
        writer.finish().unwrap();
    }

    pub fn write_empty_files(&self, count: usize) {
        let file = File::create(&self.archive).unwrap();
        let mut writer = ZipWriter::new(file);
        for index in 0..count {
            writer
                .start_file(format!("{index}.cpp"), SimpleFileOptions::default())
                .unwrap();
        }
        writer.finish().unwrap();
    }

    pub fn write_copies(&self, count: usize, contents: &[u8]) {
        let file = File::create(&self.archive).unwrap();
        let mut writer = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for index in 0..count {
            writer.start_file(format!("{index}.cpp"), options).unwrap();
            writer.write_all(contents).unwrap();
        }
        writer.finish().unwrap();
    }

    pub fn patch_headers(&self, local_offset: usize, central_offset: usize, value: u16) {
        let mut bytes = fs::read(&self.archive).unwrap();
        patch_u16(&mut bytes, b"PK\x03\x04", local_offset, value);
        patch_u16(&mut bytes, b"PK\x01\x02", central_offset, value);
        fs::write(&self.archive, bytes).unwrap();
    }

    pub fn patch_uncompressed_size(&self, value: u32) {
        let mut bytes = fs::read(&self.archive).unwrap();
        patch_u32(&mut bytes, b"PK\x03\x04", 22, value);
        patch_u32(&mut bytes, b"PK\x01\x02", 24, value);
        fs::write(&self.archive, bytes).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

pub fn assert_violation(result: Result<impl Sized, ArchiveError>, expected: ArchiveViolation) {
    match result {
        Err(ArchiveError::Invalid(reason)) => assert_eq!(reason, expected),
        Err(error) => panic!("expected {expected:?}, got {error}"),
        Ok(_) => panic!("expected {expected:?}, got success"),
    }
}

pub fn repeated_bytes(size: usize) -> Vec<u8> {
    let mut state = 0x1234_5678_u32;
    let mut value = 0_u8;
    (0..size)
        .map(|index| {
            if index % 16 == 0 {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                value = state as u8;
            }
            value
        })
        .collect()
}

pub fn relative_files(root: &Path) -> Vec<PathBuf> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.push(path.strip_prefix(root).unwrap().to_path_buf());
            }
        }
    }
    files.sort();
    files
}

fn patch_u16(bytes: &mut [u8], signature: &[u8], offset: usize, value: u16) {
    for index in 0..bytes.len().saturating_sub(signature.len()) {
        if &bytes[index..index + signature.len()] == signature {
            bytes[index + offset..index + offset + 2].copy_from_slice(&value.to_le_bytes());
        }
    }
}

fn patch_u32(bytes: &mut [u8], signature: &[u8], offset: usize, value: u32) {
    for index in 0..bytes.len().saturating_sub(signature.len()) {
        if &bytes[index..index + signature.len()] == signature {
            bytes[index + offset..index + offset + 4].copy_from_slice(&value.to_le_bytes());
        }
    }
}
