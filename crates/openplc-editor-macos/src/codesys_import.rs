use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{Cursor, Read};
use std::sync::OnceLock;

use open_rust_plc_compiler::library::StlibArchive;
use open_rust_plc_compiler::library_compile::{
    compile_stlib, CompileStlibOptions, LibrarySourceInput,
};
use regex::Regex;
use zip::ZipArchive;

const V23_MAGIC: &[u8; 8] = b"CoDeSys+";
const ZIP_MAGIC: &[u8; 4] = b"PK\x03\x04";
const MIN_DECL_LEN: usize = 10;
const MAX_DECL_LEN: usize = 100_000;
const MAX_IMPL_LEN: usize = 500_000;
const MAX_ARCHIVE_BYTES: usize = 64 * 1024 * 1024;
const MAX_ZIP_ENTRIES: usize = 5_000;
const MAX_ZIP_ENTRY_BYTES: u64 = 16 * 1024 * 1024;
const STRING_TABLE_NAME: &str = "__shared_data_storage_string_table__.auxiliary";
const OBJECT_HEADER_SIZE: usize = 20;
const MIN_OBJECT_SIZE: usize = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodesysFormat {
    V23,
    V3,
}

#[derive(Debug, Clone)]
pub struct CodesysImportResult {
    pub format: CodesysFormat,
    pub sources: Vec<LibrarySourceInput>,
    pub global_constants: BTreeMap<String, f64>,
}

#[derive(Debug, Clone)]
struct ExtractedPou {
    kind: PouKind,
    name: String,
    declaration: String,
    implementation: String,
    category: Option<String>,
    documentation: Option<String>,
    offset: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PouKind {
    Function,
    FunctionBlock,
    Program,
    Gvl,
    Type,
}

impl PouKind {
    fn end_marker(self) -> Option<&'static str> {
        match self {
            Self::Function => Some("END_FUNCTION"),
            Self::FunctionBlock => Some("END_FUNCTION_BLOCK"),
            Self::Program => Some("END_PROGRAM"),
            Self::Gvl | Self::Type => None,
        }
    }
}

pub fn prepare_codesys_upload(bytes: &[u8], filename: &str) -> Result<StlibArchive, String> {
    let imported = import_codesys_bytes(bytes)?;
    let format_label = match imported.format {
        CodesysFormat::V23 => "V2.3",
        CodesysFormat::V3 => "V3",
    };
    let identifier = derive_identifier(filename);
    compile_stlib(
        &imported.sources,
        &CompileStlibOptions {
            name: &identifier,
            version: "1.0.0",
            namespace: &identifier
                .chars()
                .map(|character| {
                    if character.is_ascii_alphanumeric() || character == '_' {
                        character
                    } else {
                        '_'
                    }
                })
                .collect::<String>(),
            no_source: false,
            builtin: false,
            dependencies: &[],
            global_constants: imported.global_constants,
        },
    )
    .map_err(|error| format!("CODESYS {format_label} compile failed: {error}"))
}

pub fn import_codesys_bytes(bytes: &[u8]) -> Result<CodesysImportResult, String> {
    if bytes.len() > MAX_ARCHIVE_BYTES {
        return Err("CODESYS library exceeds 64 MiB input limit".to_owned());
    }
    if bytes.starts_with(V23_MAGIC) {
        return import_v23(bytes);
    }
    if bytes.starts_with(ZIP_MAGIC) {
        return import_v3(bytes);
    }
    Err(format!(
        "Unrecognized file format (not CODESYS V2.3 or V3). Magic bytes: {}",
        bytes
            .iter()
            .take(8)
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    ))
}

fn derive_identifier(filename: &str) -> String {
    let slashed = filename.replace('\\', "/");
    let base = slashed.rsplit('/').next().unwrap_or_default();
    let stem = base
        .rfind('.')
        .filter(|index| *index > 0)
        .map_or(base, |index| &base[..index]);
    let mut identifier = String::new();
    let mut pending_dash = false;
    for character in stem.chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
            if pending_dash && !identifier.is_empty() {
                identifier.push('-');
            }
            pending_dash = false;
            identifier.push(character);
        } else {
            pending_dash = true;
        }
    }
    let identifier = identifier.trim_start_matches(['.', '-']).to_owned();
    if identifier.is_empty() {
        "imported-library".to_owned()
    } else {
        identifier
    }
}

fn import_v23(bytes: &[u8]) -> Result<CodesysImportResult, String> {
    let mut pous = find_v23_records(bytes);
    if pous.is_empty() {
        return Err("No POUs found in library file.".to_owned());
    }
    let mut constants = BTreeMap::new();
    pous.retain(|pou| {
        if pou.kind != PouKind::Gvl {
            return true;
        }
        if let Some(found) = extract_constant_globals(&pou.declaration) {
            constants.extend(found);
            false
        } else {
            true
        }
    });
    Ok(CodesysImportResult {
        format: CodesysFormat::V23,
        sources: pous.into_iter().map(format_source).collect(),
        global_constants: constants,
    })
}

fn find_v23_records(bytes: &[u8]) -> Vec<ExtractedPou> {
    let patterns: [(&[u8], PouKind, bool); 5] = [
        (b"FUNCTION_BLOCK ", PouKind::FunctionBlock, true),
        (b"FUNCTION ", PouKind::Function, true),
        (b"PROGRAM ", PouKind::Program, true),
        (b"VAR_GLOBAL", PouKind::Gvl, false),
        (b"TYPE ", PouKind::Type, false),
    ];
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut gvl_index = 0usize;
    for (pattern, kind, has_impl) in patterns {
        let mut offset = 0usize;
        while let Some(relative) = find_bytes(&bytes[offset..], pattern) {
            let index = offset + relative;
            offset = index + 1;
            let text_start = leading_whitespace_start(bytes, index);
            if !seen.insert(text_start) {
                continue;
            }
            let Some((declaration, end)) = read_v23_decl(bytes, text_start) else {
                seen.remove(&text_start);
                continue;
            };
            let name = match kind {
                PouKind::Gvl => {
                    if !declaration.trim_start().starts_with("VAR_GLOBAL") {
                        continue;
                    }
                    let name = recover_gvl_name(bytes, text_start, gvl_index);
                    gvl_index += 1;
                    name
                }
                PouKind::Type => type_name(&declaration).unwrap_or_default(),
                _ => pou_name(&declaration).unwrap_or_default(),
            };
            if name.is_empty() {
                continue;
            }
            out.push(ExtractedPou {
                kind,
                name,
                declaration: declaration.trim_start().to_owned(),
                implementation: if has_impl {
                    read_v23_impl(bytes, end).unwrap_or_default()
                } else {
                    String::new()
                },
                category: None,
                documentation: None,
                offset: text_start,
            });
        }
    }
    out.sort_by_key(|pou| pou.offset);
    out
}

fn leading_whitespace_start(bytes: &[u8], index: usize) -> usize {
    let mut start = index;
    for lookback in 1..=2 {
        if index >= lookback && matches!(bytes[index - lookback], b'\t' | b' ') {
            start = index - lookback;
        } else {
            break;
        }
    }
    start
}

fn read_v23_decl(bytes: &[u8], start: usize) -> Option<(String, usize)> {
    if start < 4 {
        return None;
    }
    let length = read_u32_le(bytes, start - 4)? as usize;
    if !(MIN_DECL_LEN..=MAX_DECL_LEN).contains(&length) || start.checked_add(length)? > bytes.len()
    {
        return None;
    }
    let text = latin1(&bytes[start..start + length]);
    clean_text(&text).then_some((text, start + length))
}

fn read_v23_impl(bytes: &[u8], decl_end: usize) -> Option<String> {
    if bytes.get(decl_end).copied()? != 0x12 || decl_end + 5 > bytes.len() {
        return None;
    }
    let length = read_u32_le(bytes, decl_end + 1)? as usize;
    if length == 0 || length > MAX_IMPL_LEN || decl_end + 5 + length > bytes.len() {
        return None;
    }
    let text = latin1(&bytes[decl_end + 5..decl_end + 5 + length]);
    clean_text(&text).then_some(text)
}

fn clean_text(text: &str) -> bool {
    let mut cd_run = 0;
    for character in text.chars() {
        let code = character as u32;
        if code == 0 || (code < 0x20 && !matches!(code, 0x09 | 0x0a | 0x0d)) {
            return false;
        }
        cd_run = if code == 0xcd { cd_run + 1 } else { 0 };
        if cd_run >= 3 {
            return false;
        }
    }
    true
}

fn recover_gvl_name(bytes: &[u8], record_start: usize, fallback: usize) -> String {
    let lower = record_start.saturating_sub(400);
    for position in (lower..record_start.saturating_sub(4)).rev() {
        let Some(length) = read_u32_le(bytes, position).map(|value| value as usize) else {
            continue;
        };
        if !(1..=64).contains(&length) || position + 4 + length > record_start {
            continue;
        }
        let candidate = &bytes[position + 4..position + 4 + length];
        if candidate
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        {
            return String::from_utf8_lossy(candidate).into_owned();
        }
    }
    format!("GVL_{fallback}")
}

fn import_v3(bytes: &[u8]) -> Result<CodesysImportResult, String> {
    let entries = unzip_entries(bytes)?;
    let string_table = entries
        .get(STRING_TABLE_NAME)
        .ok_or_else(|| "String table not found in ZIP archive.".to_owned())?;
    let (strings, _guid) = parse_string_table(string_table)?;
    let folder_paths = build_folder_paths(&entries, &strings);
    let type_map = build_type_map(&strings);
    let mut pous = Vec::new();
    let mut seen_names = HashSet::new();
    let mut constants = BTreeMap::new();
    let mut gvl_counter = 0usize;
    let mut object_names = entries
        .keys()
        .filter(|name| name.ends_with(".object"))
        .cloned()
        .collect::<Vec<_>>();
    object_names.sort();
    for name in object_names {
        let object = &entries[&name];
        if object.len() < MIN_OBJECT_SIZE {
            continue;
        }
        let records = parse_object_records(object);
        let Some((declaration, implementation, documentation)) =
            extract_from_records(&records, &strings)
        else {
            continue;
        };
        if let Some(found) = extract_constant_globals(&declaration) {
            constants.extend(found);
            continue;
        }
        let mut pou = classify_pou(&declaration, &implementation, &mut gvl_counter)
            .or_else(|| handle_bare_struct(&declaration, &type_map))
            .or_else(|| handle_bare_gvl(&declaration, &mut gvl_counter));
        let Some(mut pou) = pou.take() else {
            continue;
        };
        let key = pou.name.to_ascii_uppercase();
        if !seen_names.insert(key) {
            continue;
        }
        let object_guid = name.trim_end_matches(".object");
        if let Some(path) = folder_paths
            .get(object_guid)
            .filter(|path| !path.is_empty())
        {
            pou.category = strip_codesys_category_prefix(path);
        }
        pou.documentation = documentation;
        pous.push(pou);
    }
    pous.sort_by(|left, right| left.name.cmp(&right.name));
    if pous.is_empty() {
        return Err("No POUs found in library archive.".to_owned());
    }
    Ok(CodesysImportResult {
        format: CodesysFormat::V3,
        sources: pous.into_iter().map(format_source).collect(),
        global_constants: constants,
    })
}

fn unzip_entries(bytes: &[u8]) -> Result<HashMap<String, Vec<u8>>, String> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| format!("open CODESYS V3 ZIP: {error}"))?;
    if archive.len() > MAX_ZIP_ENTRIES {
        return Err(format!(
            "CODESYS V3 ZIP has too many entries: {}",
            archive.len()
        ));
    }
    archive
        .validate_header_consistency()
        .map_err(|error| format!("inconsistent CODESYS V3 ZIP headers: {error}"))?;
    let mut total = 0u64;
    let mut entries = HashMap::new();
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|error| format!("read CODESYS V3 ZIP entry {index}: {error}"))?;
        if file.is_dir() {
            continue;
        }
        let name = file
            .name()
            .map_err(|error| format!("read CODESYS V3 ZIP entry name: {error}"))?
            .into_owned();
        if file.size() > MAX_ZIP_ENTRY_BYTES {
            return Err(format!("CODESYS V3 ZIP entry {} exceeds size limit", name));
        }
        total = total
            .checked_add(file.size())
            .ok_or_else(|| "CODESYS V3 ZIP size overflow".to_owned())?;
        if total > MAX_ARCHIVE_BYTES as u64 {
            return Err("CODESYS V3 ZIP exceeds uncompressed size limit".to_owned());
        }
        if name.contains("../") || name.contains("..\\") || name.starts_with(['/', '\\']) {
            return Err(format!("unsafe CODESYS V3 ZIP entry path {name:?}"));
        }
        let mut data = Vec::with_capacity(file.size() as usize);
        file.read_to_end(&mut data)
            .map_err(|error| format!("read CODESYS V3 ZIP entry {name}: {error}"))?;
        entries.insert(name, data);
    }
    Ok(entries)
}

fn parse_string_table(bytes: &[u8]) -> Result<(HashMap<u32, String>, String), String> {
    if bytes.len() < 4 || bytes[..2] != [0xfa, 0x53] {
        return Err("Invalid CODESYS V3 string table magic".to_owned());
    }
    let guid_length = bytes[3] as usize;
    if 4 + guid_length > bytes.len() {
        return Err("CODESYS V3 string table GUID is truncated".to_owned());
    }
    let guid = String::from_utf8_lossy(&bytes[4..4 + guid_length]).into_owned();
    let mut offset = 4 + guid_length;
    let mut strings = HashMap::new();
    while offset < bytes.len() {
        let Some((index, after_index)) = read_leb128(bytes, offset) else {
            break;
        };
        let Some((length, after_length)) = read_leb128(bytes, after_index) else {
            break;
        };
        offset = after_length;
        let length = length as usize;
        if offset + length > bytes.len() {
            break;
        }
        strings.insert(
            index,
            String::from_utf8_lossy(&bytes[offset..offset + length]).into_owned(),
        );
        offset += length;
    }
    Ok((strings, guid))
}

fn read_leb128(bytes: &[u8], mut offset: usize) -> Option<(u32, usize)> {
    let mut value = 0u32;
    let mut shift = 0u32;
    while offset < bytes.len() {
        let byte = bytes[offset];
        offset += 1;
        value |= u32::from(byte & 0x7f).checked_shl(shift)?;
        if byte & 0x80 == 0 {
            return Some((value, offset));
        }
        shift += 7;
        if shift >= 28 {
            return Some((value, offset));
        }
    }
    None
}

fn decode_varints_after_header(bytes: &[u8]) -> Vec<u32> {
    if bytes.len() < OBJECT_HEADER_SIZE {
        return Vec::new();
    }
    let mut values = Vec::new();
    let mut offset = OBJECT_HEADER_SIZE;
    while let Some((value, next)) = read_leb128(bytes, offset) {
        values.push(value);
        if next <= offset {
            break;
        }
        offset = next;
        if offset >= bytes.len() {
            break;
        }
    }
    values
}

fn parse_object_records(bytes: &[u8]) -> Vec<Vec<u32>> {
    if bytes.len() < OBJECT_HEADER_SIZE {
        return Vec::new();
    }
    let mut offset = OBJECT_HEADER_SIZE;
    let mut records = Vec::new();
    let mut current = Vec::new();
    while offset < bytes.len() {
        let Some((value, next)) = read_leb128(bytes, offset) else {
            break;
        };
        current.push(value);
        offset = next;
        if offset + 8 <= bytes.len() && bytes[offset..offset + 8].iter().all(|byte| *byte == 0) {
            records.push(std::mem::take(&mut current));
            offset += 8;
        }
    }
    if !current.is_empty() {
        records.push(current);
    }
    records
}

#[derive(Debug, Clone)]
struct MetaInfo {
    display_name: String,
    guid_refs: Vec<String>,
}

fn build_folder_paths(
    entries: &HashMap<String, Vec<u8>>,
    strings: &HashMap<u32, String>,
) -> HashMap<String, String> {
    let mut meta_by_guid = HashMap::new();
    let mut ref_count = HashMap::<String, usize>::new();
    for (name, bytes) in entries {
        let Some(guid) = name.strip_suffix(".meta") else {
            continue;
        };
        let info = parse_meta_info(bytes, guid, strings);
        for reference in &info.guid_refs {
            *ref_count.entry(reference.clone()).or_default() += 1;
        }
        meta_by_guid.insert(guid.to_owned(), info);
    }
    let threshold = meta_by_guid.len() / 2;
    let ambient = ref_count
        .into_iter()
        .filter_map(|(guid, count)| (count > threshold).then_some(guid))
        .collect::<HashSet<_>>();
    let mut parent_by_guid = HashMap::new();
    for (guid, info) in &meta_by_guid {
        if let Some(parent) = info
            .guid_refs
            .iter()
            .find(|reference| !ambient.contains(*reference))
        {
            parent_by_guid.insert(guid.clone(), parent.clone());
        }
    }
    let mut paths = HashMap::new();
    for guid in meta_by_guid.keys() {
        let mut segments = Vec::new();
        let mut seen = HashSet::new();
        let mut current = parent_by_guid.get(guid).cloned();
        while let Some(parent) = current {
            if !seen.insert(parent.clone()) {
                break;
            }
            let Some(meta) = meta_by_guid.get(&parent) else {
                break;
            };
            if !meta.display_name.is_empty() {
                segments.push(meta.display_name.clone());
            }
            current = parent_by_guid.get(&parent).cloned();
        }
        segments.reverse();
        paths.insert(guid.clone(), segments.join("/"));
    }
    paths
}

fn parse_meta_info(bytes: &[u8], own_guid: &str, strings: &HashMap<u32, String>) -> MetaInfo {
    let mut guid_refs = Vec::new();
    let mut display_name = String::new();
    for (index, value) in decode_varints_after_header(bytes).into_iter().enumerate() {
        let Some(text) = strings.get(&value) else {
            continue;
        };
        if is_guid(text) {
            if text != own_guid && text != "00000000-0000-0000-0000-000000000000" {
                guid_refs.push(text.clone());
            }
        } else if index == 4 && display_name.is_empty() && !text.is_empty() {
            display_name = text.clone();
        }
    }
    MetaInfo {
        display_name,
        guid_refs,
    }
}

fn is_guid(text: &str) -> bool {
    static GUID: OnceLock<Regex> = OnceLock::new();
    GUID.get_or_init(|| {
        Regex::new(r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$")
            .expect("static GUID regex")
    })
    .is_match(text)
}

fn build_type_map(strings: &HashMap<u32, String>) -> HashMap<String, String> {
    let mut indices = strings.keys().copied().collect::<Vec<_>>();
    indices.sort_unstable();
    let mut result = HashMap::new();
    for (position, index) in indices.iter().enumerate() {
        let Some(text) = strings.get(index) else {
            continue;
        };
        let Some(name) = type_name(text) else {
            continue;
        };
        for next in indices.iter().skip(position + 1).take(4) {
            let candidate = strings.get(next).map(String::as_str).unwrap_or_default();
            if candidate.trim().starts_with("STRUCT") {
                result.insert(candidate.to_owned(), name.clone());
                break;
            }
        }
    }
    result
}

fn extract_from_records(
    records: &[Vec<u32>],
    strings: &HashMap<u32, String>,
) -> Option<(String, String, Option<String>)> {
    if records.len() < 3 {
        return None;
    }
    let total_impl = (records[0].len() == 9)
        .then(|| records[0].get(5).copied())
        .flatten()
        .unwrap_or(0) as usize;
    let boundary = records
        .iter()
        .enumerate()
        .skip(1)
        .find_map(|(index, record)| (record.len() > 3).then_some(index))?;
    let mut impl_lines = records[1..boundary]
        .iter()
        .filter_map(|record| record.first().and_then(|value| strings.get(value)).cloned())
        .collect::<Vec<_>>();
    let mut cursor = boundary;
    if total_impl > impl_lines.len() {
        impl_lines.extend(extract_boundary_rows(
            &records[boundary],
            strings,
            total_impl - impl_lines.len(),
        ));
        cursor += 1;
    }
    let mut decl_lines = Vec::new();
    let mut header_index = None;
    for (index, record) in records.iter().enumerate().skip(cursor) {
        if let Some(header) = find_header(record, strings) {
            decl_lines.push(header);
            header_index = Some(index);
            break;
        }
        if impl_lines.len() < total_impl {
            if let Some(text) = record.first().and_then(|value| strings.get(value)) {
                impl_lines.push(text.clone());
            }
        }
    }
    let header_index = header_index?;
    for record in records.iter().skip(header_index + 1) {
        if let Some(text) = record.first().and_then(|value| strings.get(value)) {
            decl_lines.push(text.clone());
        }
    }
    let documentation = extract_documentation(records, header_index + 1, strings);
    Some((decl_lines.join("\n"), impl_lines.join("\n"), documentation))
}

fn extract_boundary_rows(
    record: &[u32],
    strings: &HashMap<u32, String>,
    expected: usize,
) -> Vec<String> {
    let mut lines = Vec::new();
    let mut position = 0usize;
    while position < record.len() && lines.len() < expected {
        lines.push(strings.get(&record[position]).cloned().unwrap_or_default());
        position += 1;
        while position < record.len() && record[position] != 0 {
            position += 1;
        }
        let mut zeros = 0;
        while position < record.len() && record[position] == 0 {
            position += 1;
            zeros += 1;
        }
        if zeros == 0 && position < record.len() {
            break;
        }
    }
    lines
}

fn find_header(record: &[u32], strings: &HashMap<u32, String>) -> Option<String> {
    if record.len() == 4 {
        if let Some(text) = record.get(1).and_then(|value| strings.get(value)) {
            if is_header(text) {
                return Some(text.clone());
            }
        }
    }
    let text = record.first().and_then(|value| strings.get(value))?;
    is_header(text).then(|| text.clone())
}

fn is_header(text: &str) -> bool {
    pou_name(text).is_some()
        || type_name(text).is_some()
        || text.trim_start().starts_with("VAR_GLOBAL")
}

fn extract_documentation(
    records: &[Vec<u32>],
    start: usize,
    strings: &HashMap<u32, String>,
) -> Option<String> {
    let mut anchor = None;
    for (index, record) in records.iter().enumerate().skip(start) {
        let line = record
            .first()
            .and_then(|value| strings.get(value))
            .map(|text| text.trim())
            .unwrap_or_default();
        if matches!(line, "END_VAR" | "END_TYPE") {
            anchor = Some(index);
        }
    }
    let anchor = anchor?;
    let tail = records
        .iter()
        .skip(anchor + 1)
        .filter_map(|record| record.first().and_then(|value| strings.get(value)))
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    let start = tail.find("(*")? + 2;
    let end = tail[start..].find("*)")? + start;
    let documentation = tail[start..end].trim();
    (!documentation.is_empty()).then(|| documentation.to_owned())
}

fn classify_pou(
    declaration: &str,
    implementation: &str,
    gvl_counter: &mut usize,
) -> Option<ExtractedPou> {
    for line in declaration.lines() {
        if let Some((kind, name)) = pou_kind_and_name(line) {
            return Some(ExtractedPou {
                kind,
                name,
                declaration: declaration.to_owned(),
                implementation: implementation.to_owned(),
                category: None,
                documentation: None,
                offset: 0,
            });
        }
        if let Some(name) = type_name(line) {
            return Some(ExtractedPou {
                kind: PouKind::Type,
                name,
                declaration: declaration.to_owned(),
                implementation: String::new(),
                category: None,
                documentation: None,
                offset: 0,
            });
        }
        if line.trim_start().starts_with("VAR_GLOBAL") {
            if !declaration.lines().any(is_variable_line) {
                return None;
            }
            let name = format!("GVL_{}", *gvl_counter);
            *gvl_counter += 1;
            return Some(ExtractedPou {
                kind: PouKind::Gvl,
                name,
                declaration: declaration.to_owned(),
                implementation: String::new(),
                category: None,
                documentation: None,
                offset: 0,
            });
        }
    }
    None
}

fn handle_bare_struct(
    declaration: &str,
    type_map: &HashMap<String, String>,
) -> Option<ExtractedPou> {
    let first = declaration.lines().next().unwrap_or_default();
    if !first.trim().starts_with("STRUCT") {
        return None;
    }
    let name = type_map.get(first)?.clone();
    Some(ExtractedPou {
        kind: PouKind::Type,
        name: name.clone(),
        declaration: format!("TYPE {name} :\n{declaration}\nEND_TYPE"),
        implementation: String::new(),
        category: None,
        documentation: None,
        offset: 0,
    })
}

fn handle_bare_gvl(declaration: &str, gvl_counter: &mut usize) -> Option<ExtractedPou> {
    let trimmed = declaration.trim();
    let plausible = trimmed.starts_with("VAR_GLOBAL")
        || trimmed.starts_with("VAR_INPUT")
        || declaration.lines().any(is_variable_line);
    if !plausible || !declaration.lines().any(is_variable_line) {
        return None;
    }
    let name = format!("GVL_{}", *gvl_counter);
    *gvl_counter += 1;
    let body = if trimmed.starts_with("VAR_GLOBAL") || trimmed.starts_with("VAR_INPUT") {
        declaration.to_owned()
    } else {
        format!("VAR_GLOBAL\n{declaration}\nEND_VAR")
    };
    Some(ExtractedPou {
        kind: PouKind::Gvl,
        name,
        declaration: body,
        implementation: String::new(),
        category: None,
        documentation: None,
        offset: 0,
    })
}

fn strip_codesys_category_prefix(path: &str) -> Option<String> {
    if path == "POUs" {
        None
    } else if let Some(path) = path.strip_prefix("POUs/") {
        (!path.is_empty()).then(|| path.to_owned())
    } else {
        Some(path.to_owned())
    }
}

fn extract_constant_globals(declaration: &str) -> Option<BTreeMap<String, f64>> {
    static CONSTANT: OnceLock<Regex> = OnceLock::new();
    let regex = CONSTANT.get_or_init(|| {
        Regex::new(r"^(\w+)\s*:\s*(\w+)\s*:=\s*([+-]?\d+)\s*;?\s*$").expect("static constant regex")
    });
    let integer_types = [
        "BYTE", "SINT", "USINT", "WORD", "INT", "UINT", "DWORD", "DINT", "UDINT", "LWORD", "LINT",
        "ULINT",
    ];
    let mut in_constant = false;
    let mut constants = BTreeMap::new();
    for raw in declaration.lines() {
        let line = raw.trim();
        if line.is_empty()
            || line.starts_with("(*")
            || line.starts_with("//")
            || line.starts_with('*')
        {
            continue;
        }
        if line.starts_with("VAR_GLOBAL") && line.contains("CONSTANT") {
            in_constant = true;
            continue;
        }
        if line == "END_VAR" {
            in_constant = false;
            continue;
        }
        if !in_constant {
            continue;
        }
        let captures = regex.captures(line)?;
        let type_name = captures.get(2)?.as_str().to_ascii_uppercase();
        if !integer_types.contains(&type_name.as_str()) {
            return None;
        }
        let value = captures.get(3)?.as_str().parse::<i64>().ok()?;
        constants.insert(captures.get(1)?.as_str().to_owned(), value as f64);
    }
    (!constants.is_empty()).then_some(constants)
}

fn format_source(pou: ExtractedPou) -> LibrarySourceInput {
    let declaration = pou.declaration.replace("\r\n", "\n").trim_end().to_owned();
    let implementation = pou
        .implementation
        .replace("\r\n", "\n")
        .trim_end()
        .to_owned();
    let source = if let Some(end) = pou.kind.end_marker() {
        let mut parts = vec![declaration];
        if !implementation.is_empty() {
            parts.push(String::new());
            parts.push(implementation);
        }
        parts.push(String::new());
        parts.push(end.to_owned());
        parts.push(String::new());
        parts.join("\n")
    } else {
        declaration + "\n"
    };
    LibrarySourceInput {
        file_name: if pou.kind == PouKind::Gvl {
            format!("{}.gvl.st", pou.name)
        } else {
            format!("{}.st", pou.name)
        },
        source,
        category: pou.category,
        documentation: pou.documentation,
    }
}

fn pou_name(text: &str) -> Option<String> {
    pou_kind_and_name(text).map(|(_, name)| name)
}

fn pou_kind_and_name(text: &str) -> Option<(PouKind, String)> {
    static POU: OnceLock<Regex> = OnceLock::new();
    let captures = POU
        .get_or_init(|| {
            Regex::new(r"^\s*(FUNCTION_BLOCK|FUNCTION|PROGRAM)\s+(\w+)").expect("static POU regex")
        })
        .captures(text)?;
    let kind = match captures.get(1)?.as_str() {
        "FUNCTION_BLOCK" => PouKind::FunctionBlock,
        "FUNCTION" => PouKind::Function,
        "PROGRAM" => PouKind::Program,
        _ => return None,
    };
    Some((kind, captures.get(2)?.as_str().to_owned()))
}

fn type_name(text: &str) -> Option<String> {
    static TYPE: OnceLock<Regex> = OnceLock::new();
    TYPE.get_or_init(|| Regex::new(r"^\s*TYPE\s+(\w+)\s*:?").expect("static TYPE regex"))
        .captures(text)
        .and_then(|captures| captures.get(1).map(|name| name.as_str().to_owned()))
}

fn is_variable_line(line: &str) -> bool {
    static VARIABLE: OnceLock<Regex> = OnceLock::new();
    VARIABLE
        .get_or_init(|| Regex::new(r"^\s*\w+\s*:").expect("static variable regex"))
        .is_match(line)
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn read_u32_le(bytes: &[u8], offset: usize) -> Option<u32> {
    let slice = bytes.get(offset..offset + 4)?;
    Some(u32::from_le_bytes(slice.try_into().ok()?))
}

fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| char::from(*byte)).collect()
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::io::Write;

    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    use super::{derive_identifier, import_codesys_bytes, prepare_codesys_upload, CodesysFormat};

    #[test]
    fn imports_v23_length_prefixed_function() {
        let declaration = b"FUNCTION AddOne : INT\r\nVAR_INPUT\r\nA : INT;\r\nEND_VAR";
        let implementation = b"AddOne := A + 1;";
        let mut bytes = b"CoDeSys+".to_vec();
        bytes.extend_from_slice(&[0xaa, 0xbb, 0xcc]);
        bytes.extend_from_slice(&(declaration.len() as u32).to_le_bytes());
        bytes.extend_from_slice(declaration);
        bytes.push(0x12);
        bytes.extend_from_slice(&(implementation.len() as u32).to_le_bytes());
        bytes.extend_from_slice(implementation);
        let imported = import_codesys_bytes(&bytes).unwrap();
        assert_eq!(imported.format, CodesysFormat::V23);
        assert_eq!(imported.sources.len(), 1);
        assert!(imported.sources[0].source.contains("END_FUNCTION"));
        assert!(imported.sources[0].source.contains("AddOne := A + 1;"));
    }

    #[test]
    fn detects_minimal_v3_string_table_archive() {
        let cursor = std::io::Cursor::new(Vec::new());
        let mut zip = ZipWriter::new(cursor);
        zip.start_file(
            "__shared_data_storage_string_table__.auxiliary",
            SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(&[0xfa, 0x53, 0, 0]).unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        let error = import_codesys_bytes(&bytes).unwrap_err();
        assert!(error.contains("No POUs"));
    }

    #[test]
    fn identifier_matches_pinned_filename_fallback() {
        assert_eq!(
            derive_identifier("C:\\tmp\\OSCAT Basic 3.35.library"),
            "OSCAT-Basic-3.35"
        );
        assert_eq!(derive_identifier("...library"), "imported-library");
    }

    #[test]
    #[ignore = "requires RUSTPLC_CODESYS_OSCAT_FIXTURE pointing at pinned oscat_basic_335.library"]
    fn pinned_oscat_v3_import_and_compile_matches_release_shape() {
        let path = env::var("RUSTPLC_CODESYS_OSCAT_FIXTURE").unwrap();
        let bytes = fs::read(&path).unwrap();
        let imported = import_codesys_bytes(&bytes).unwrap();
        assert_eq!(imported.format, CodesysFormat::V3);
        assert_eq!(imported.sources.len(), 560);
        assert_eq!(imported.global_constants.get("STRING_LENGTH"), Some(&250.0));
        assert_eq!(imported.global_constants.get("LIST_LENGTH"), Some(&250.0));
        let archive = prepare_codesys_upload(
            &bytes,
            std::path::Path::new(&path)
                .file_name()
                .unwrap()
                .to_str()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(archive.manifest.functions.len(), 373);
        assert_eq!(archive.manifest.function_blocks.len(), 172);
        assert_eq!(archive.manifest.types.len(), 14);
        assert_eq!(archive.manifest.globals.len(), 5);
        assert_eq!(archive.chunks.len(), 564);
    }
}
