use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use ring::{digest, signature};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use uuid::Uuid;
use zip::ZipArchive;

use crate::atomic_file;

const REGISTRY_FILE: &str = "registry.json";
const SIGNATURE_FILE: &str = "signature.json";
const MAX_ARCHIVE_ENTRIES: usize = 4096;
const MAX_UNCOMPRESSED_BYTES: u64 = 512 * 1024 * 1024;

// Exact raw Ed25519 public key from pinned OpenPLC trusted-keys.ts
// key id: openplc-2026
const OPENPLC_2026_PUBLIC_KEY: [u8; 32] = [
    0x05, 0xdc, 0x1e, 0x12, 0xe2, 0x40, 0x7d, 0x81, 0xbd, 0xdb, 0x74, 0x64, 0x99, 0x92, 0xd8, 0xb2,
    0x6a, 0x27, 0x2e, 0xf0, 0x9c, 0x81, 0x5b, 0x60, 0xa4, 0x9e, 0xe6, 0x9e, 0x06, 0xd1, 0x25, 0x09,
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VppManifest {
    pub format_version: String,
    pub package: VppPackageMetadata,
    pub devices: Vec<VppDevice>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VppPackageMetadata {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub vendor: VppVendor,
    #[serde(default)]
    pub description: String,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct VppVendor {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub logo: String,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VppDevice {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub preview: String,
    #[serde(default)]
    pub target: VppTarget,
    #[serde(default)]
    pub specs: BTreeMap<String, String>,
    #[serde(default)]
    pub hal: VppHal,
    #[serde(default)]
    pub defaults: VppDefaults,
    #[serde(default)]
    pub screens: BTreeMap<String, String>,
    #[serde(default)]
    pub capabilities: BTreeMap<String, Value>,
    #[serde(default)]
    pub module_system: Option<Value>,
    #[serde(default)]
    pub debug: Option<Value>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VppTarget {
    #[serde(rename = "type", default)]
    pub target_type: String,
    #[serde(default)]
    pub platform: Option<String>,
    #[serde(default)]
    pub core: Option<String>,
    #[serde(default)]
    pub board_manager_url: Option<String>,
    #[serde(default)]
    pub platform_options: Vec<VppPlatformOption>,
    #[serde(default)]
    pub core_version: Option<String>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VppPlatformOption {
    pub key: String,
    pub label: String,
    pub default: String,
    #[serde(default)]
    pub help: Option<String>,
    #[serde(default)]
    pub values: Vec<VppPlatformOptionValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VppPlatformOptionValue {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub help: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VppHal {
    #[serde(rename = "type", default)]
    pub hal_type: String,
    #[serde(default)]
    pub plugin_type: Option<String>,
    #[serde(default)]
    pub provisioning: Option<String>,
    #[serde(default)]
    pub plugin_entry: Option<String>,
    #[serde(default)]
    pub config_template: Option<String>,
    #[serde(default)]
    pub requirements: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub precompiled_library: Option<String>,
    #[serde(default)]
    pub compiler_flags: VppCompilerFlags,
    #[serde(default)]
    pub define: Option<Value>,
    #[serde(default)]
    pub extra_arduino_libraries: Vec<String>,
    #[serde(default)]
    pub libraries: Option<String>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct VppCompilerFlags {
    #[serde(default)]
    pub c_flags: Vec<String>,
    #[serde(default)]
    pub cxx_flags: Vec<String>,
    #[serde(default)]
    pub ld_flags: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct VppDefaults {
    #[serde(default, rename = "runtimeIpAddress")]
    pub runtime_ip_address: Option<String>,
    #[serde(default)]
    pub pins: VppDefaultPins,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VppDefaultPins {
    #[serde(default)]
    pub default_din: Vec<String>,
    #[serde(default)]
    pub default_dout: Vec<String>,
    #[serde(default)]
    pub default_ain: Vec<String>,
    #[serde(default)]
    pub default_aout: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InstalledVppPackage {
    pub package_id: String,
    pub version: String,
    pub installed_at: String,
    pub path: PathBuf,
    pub devices: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InstalledVppBoard {
    pub package_id: String,
    pub package_path: PathBuf,
    pub vendor: String,
    pub device: VppDevice,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PackageRegistry {
    format_version: String,
    #[serde(default)]
    packages: BTreeMap<String, RegistryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegistryEntry {
    version: String,
    installed_at: String,
    path: PathBuf,
    #[serde(default)]
    devices: Vec<String>,
}

impl Default for PackageRegistry {
    fn default() -> Self {
        Self {
            format_version: "1.0".to_owned(),
            packages: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct VppPackageManager {
    packages_dir: PathBuf,
    trusted_keys: BTreeMap<String, Vec<u8>>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SignatureDocument {
    format_version: String,
    alg: String,
    key_id: String,
    package_id: String,
    version: String,
    signed_at: String,
    files: BTreeMap<String, String>,
    signature: String,
}

impl VppPackageManager {
    pub fn new() -> Result<Self, String> {
        Ok(Self::with_packages_dir(
            crate::app_paths::data_root()?.join("packages"),
        ))
    }

    pub fn with_packages_dir(packages_dir: PathBuf) -> Self {
        let mut trusted_keys = BTreeMap::new();
        trusted_keys.insert("openplc-2026".to_owned(), OPENPLC_2026_PUBLIC_KEY.to_vec());
        Self {
            packages_dir,
            trusted_keys,
        }
    }

    #[cfg(test)]
    fn with_packages_dir_and_keys(
        packages_dir: PathBuf,
        trusted_keys: BTreeMap<String, Vec<u8>>,
    ) -> Self {
        Self {
            packages_dir,
            trusted_keys,
        }
    }

    pub fn import_from_file(&self, vpp_path: &Path) -> Result<InstalledVppPackage, String> {
        if !vpp_path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("vpp"))
        {
            return Err("Unsupported package format; expected a .vpp package.".to_owned());
        }
        fs::create_dir_all(&self.packages_dir)
            .map_err(|error| format!("create {}: {error}", self.packages_dir.display()))?;
        let temp_dir = self
            .packages_dir
            .join(format!(".import-{}", Uuid::new_v4()));
        fs::create_dir(&temp_dir)
            .map_err(|error| format!("create {}: {error}", temp_dir.display()))?;

        let result = (|| {
            extract_vpp(vpp_path, &temp_dir)?;
            verify_package_signature(&temp_dir, &self.trusted_keys)?;
            let manifest = read_manifest(&temp_dir)?;
            validate_manifest(&manifest)?;
            validate_path_id(&manifest.package.id, "manifest.package.id")?;

            let target_dir = self.packages_dir.join(&manifest.package.id);
            assert_contained(&self.packages_dir, &target_dir, "package install path")?;
            if target_dir.exists() {
                fs::remove_dir_all(&target_dir)
                    .map_err(|error| format!("remove {}: {error}", target_dir.display()))?;
            }
            fs::rename(&temp_dir, &target_dir).map_err(|error| {
                format!(
                    "promote {} to {}: {error}",
                    temp_dir.display(),
                    target_dir.display()
                )
            })?;

            let installed = InstalledVppPackage {
                package_id: manifest.package.id.clone(),
                version: manifest.package.version.clone(),
                installed_at: OffsetDateTime::now_utc()
                    .format(&Rfc3339)
                    .map_err(|error| format!("format install timestamp: {error}"))?,
                path: target_dir.clone(),
                devices: manifest
                    .devices
                    .iter()
                    .map(|device| device.id.clone())
                    .collect(),
            };
            let mut registry = self.read_registry();
            registry.packages.insert(
                installed.package_id.clone(),
                RegistryEntry {
                    version: installed.version.clone(),
                    installed_at: installed.installed_at.clone(),
                    path: installed.path.clone(),
                    devices: installed.devices.clone(),
                },
            );
            self.write_registry(&registry)?;
            Ok(installed)
        })();

        if temp_dir.exists() {
            let _ = fs::remove_dir_all(&temp_dir);
        }
        result
    }

    pub fn list_installed(&self) -> Vec<InstalledVppPackage> {
        self.read_registry()
            .packages
            .into_iter()
            .filter_map(|(package_id, entry)| {
                validate_path_id(&package_id, "registry package id").ok()?;
                assert_contained(&self.packages_dir, &entry.path, "registry package path").ok()?;
                entry.path.is_dir().then_some(InstalledVppPackage {
                    package_id,
                    version: entry.version,
                    installed_at: entry.installed_at,
                    path: entry.path,
                    devices: entry.devices,
                })
            })
            .collect()
    }

    pub fn get_manifest(&self, package_id: &str) -> Option<VppManifest> {
        validate_path_id(package_id, "packageId").ok()?;
        let entry = self.read_registry().packages.remove(package_id)?;
        assert_contained(&self.packages_dir, &entry.path, "registry package path").ok()?;
        read_manifest(&entry.path).ok()
    }

    pub fn installed_boards(&self) -> Vec<InstalledVppBoard> {
        let mut boards = Vec::new();
        for package in self.list_installed() {
            let Some(manifest) = self.get_manifest(&package.package_id) else {
                continue;
            };
            let vendor = if manifest.package.vendor.name.trim().is_empty() {
                manifest.package.name.clone()
            } else {
                manifest.package.vendor.name.clone()
            };
            for device in manifest.devices {
                if device.name.trim().is_empty() || device.id.trim().is_empty() {
                    continue;
                }
                boards.push(InstalledVppBoard {
                    package_id: package.package_id.clone(),
                    package_path: package.path.clone(),
                    vendor: vendor.clone(),
                    device,
                });
            }
        }
        boards.sort_by(|left, right| {
            left.package_id
                .cmp(&right.package_id)
                .then_with(|| left.device.name.cmp(&right.device.name))
        });
        boards
    }

    pub fn board(&self, board_name: &str) -> Option<InstalledVppBoard> {
        self.installed_boards()
            .into_iter()
            .find(|board| board.device.name == board_name)
    }

    pub fn screen_names(&self, board_name: &str) -> Vec<String> {
        let Some(board) = self.board(board_name) else {
            return Vec::new();
        };
        board.device.screens.keys().cloned().collect()
    }

    pub fn screen_definition(&self, board_name: &str, screen_name: &str) -> Result<Value, String> {
        let board = self
            .board(board_name)
            .ok_or_else(|| format!("VPP board {board_name:?} is not installed."))?;
        let relative = board.device.screens.get(screen_name).ok_or_else(|| {
            format!("VPP screen {screen_name:?} is not declared by board {board_name:?}.")
        })?;
        read_package_json(&board.package_path, relative, "VPP screen")
    }

    pub fn module_system_definition(&self, board_name: &str) -> Result<Option<Value>, String> {
        let Some(board) = self.board(board_name) else {
            return Ok(None);
        };
        let Some(mut module_system) = board.device.module_system.clone() else {
            return Ok(None);
        };
        let Some(modules) = module_system
            .get_mut("modules")
            .and_then(Value::as_array_mut)
        else {
            return Ok(Some(module_system));
        };
        for module in modules {
            let Some(object) = module.as_object_mut() else {
                continue;
            };
            let Some(relative) = object
                .get("configScreen")
                .and_then(Value::as_str)
                .map(str::to_owned)
            else {
                continue;
            };
            if let Ok(definition) = read_package_json(
                &board.package_path,
                &relative,
                "VPP module configuration screen",
            ) {
                object.insert("configScreenDefinition".to_owned(), definition);
            }
        }
        Ok(Some(module_system))
    }

    pub fn uninstall(&self, package_id: &str) -> Result<(), String> {
        validate_path_id(package_id, "packageId")?;
        let mut registry = self.read_registry();
        let entry = registry
            .packages
            .remove(package_id)
            .ok_or_else(|| format!("Package {package_id} is not installed"))?;
        assert_contained(&self.packages_dir, &entry.path, "registry package path")?;
        if entry.path.exists() {
            fs::remove_dir_all(&entry.path)
                .map_err(|error| format!("remove {}: {error}", entry.path.display()))?;
        }
        self.write_registry(&registry)
    }

    pub fn verify_installed_signatures(&self) -> Vec<String> {
        let mut registry = self.read_registry();
        let mut removed = Vec::new();
        for (package_id, entry) in registry.packages.clone() {
            let valid = validate_path_id(&package_id, "registry package id").is_ok()
                && assert_contained(&self.packages_dir, &entry.path, "registry package path")
                    .is_ok()
                && entry.path.is_dir()
                && verify_package_signature(&entry.path, &self.trusted_keys).is_ok();
            if valid {
                continue;
            }
            if assert_contained(&self.packages_dir, &entry.path, "registry package path").is_ok()
                && entry.path.is_dir()
            {
                let _ = fs::remove_dir_all(&entry.path);
            }
            registry.packages.remove(&package_id);
            removed.push(package_id);
        }
        if !removed.is_empty() {
            let _ = self.write_registry(&registry);
        }
        removed
    }

    fn read_registry(&self) -> PackageRegistry {
        let path = self.packages_dir.join(REGISTRY_FILE);
        let Ok(raw) = fs::read_to_string(path) else {
            return PackageRegistry::default();
        };
        serde_json::from_str(&raw).unwrap_or_default()
    }

    fn write_registry(&self, registry: &PackageRegistry) -> Result<(), String> {
        fs::create_dir_all(&self.packages_dir)
            .map_err(|error| format!("create {}: {error}", self.packages_dir.display()))?;
        let path = self.packages_dir.join(REGISTRY_FILE);
        let mut raw = serde_json::to_string_pretty(registry)
            .map_err(|error| format!("serialize {}: {error}", path.display()))?;
        raw.push('\n');
        atomic_file::write(&path, raw).map_err(|error| format!("write {}: {error}", path.display()))
    }
}

fn validate_manifest(manifest: &VppManifest) -> Result<(), String> {
    if manifest.format_version.trim().is_empty() {
        return Err("Invalid manifest schema: formatVersion must be non-empty".to_owned());
    }
    if manifest.package.id.trim().is_empty()
        || manifest.package.name.trim().is_empty()
        || manifest.package.version.trim().is_empty()
    {
        return Err(
            "Invalid manifest schema: package id/name/version must be non-empty".to_owned(),
        );
    }
    if manifest.devices.is_empty() {
        return Err("Invalid manifest schema: devices must contain at least one entry".to_owned());
    }
    Ok(())
}

fn read_manifest(root: &Path) -> Result<VppManifest, String> {
    let path = root.join("manifest.json");
    let raw = fs::read_to_string(&path)
        .map_err(|_| "Package does not contain a readable manifest.json file".to_owned())?;
    serde_json::from_str(&raw).map_err(|error| format!("Invalid manifest.json format: {error}"))
}

fn extract_vpp(path: &Path, destination: &Path) -> Result<(), String> {
    let input = File::open(path).map_err(|error| format!("open {}: {error}", path.display()))?;
    let mut archive = ZipArchive::new(input)
        .map_err(|error| format!("Invalid VPP ZIP {}: {error}", path.display()))?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(format!(
            "VPP contains {} entries; maximum is {MAX_ARCHIVE_ENTRIES}",
            archive.len()
        ));
    }
    archive
        .validate_header_consistency()
        .map_err(|error| format!("Inconsistent VPP ZIP headers: {error}"))?;
    let mut total = 0u64;
    let mut seen = BTreeSet::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("read VPP entry {index}: {error}"))?;
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| format!("Unsafe VPP path {:?}", entry.name()))?;
        let normalized = relative.to_string_lossy().replace('\\', "/");
        if normalized.is_empty() || !seen.insert(normalized.clone()) {
            return Err(format!("Duplicate/empty VPP path {normalized:?}"));
        }
        if entry.is_symlink() {
            return Err(format!("VPP symlink entries are not allowed: {normalized}"));
        }
        let output = destination.join(&relative);
        assert_contained(destination, &output, "VPP archive entry")?;
        if entry.is_dir() {
            fs::create_dir_all(&output)
                .map_err(|error| format!("create {}: {error}", output.display()))?;
            continue;
        }
        total = total
            .checked_add(entry.size())
            .ok_or_else(|| "VPP uncompressed size overflow".to_owned())?;
        if total > MAX_UNCOMPRESSED_BYTES {
            return Err(format!(
                "VPP uncompressed size exceeds {MAX_UNCOMPRESSED_BYTES} bytes"
            ));
        }
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("create {}: {error}", parent.display()))?;
        }
        let mut file = File::create(&output)
            .map_err(|error| format!("create {}: {error}", output.display()))?;
        io::copy(&mut entry, &mut file)
            .map_err(|error| format!("extract {normalized}: {error}"))?;
    }
    Ok(())
}

fn verify_package_signature(
    root: &Path,
    trusted_keys: &BTreeMap<String, Vec<u8>>,
) -> Result<(), String> {
    let path = root.join(SIGNATURE_FILE);
    let raw = fs::read_to_string(&path)
        .map_err(|_| "Package is not signed (missing or unreadable signature.json)".to_owned())?;
    let document: SignatureDocument =
        serde_json::from_str(&raw).map_err(|_| "signature.json is malformed".to_owned())?;
    if document.alg != "ed25519" {
        return Err(format!("Unsupported signature algorithm: {}", document.alg));
    }
    let key = trusted_keys
        .get(&document.key_id)
        .ok_or_else(|| format!("Untrusted signing key: {}", document.key_id))?;
    let signature = BASE64_STANDARD
        .decode(document.signature.as_bytes())
        .map_err(|_| "signature.json is malformed".to_owned())?;

    let payload = serde_json::json!({
        "formatVersion": document.format_version,
        "alg": document.alg,
        "keyId": document.key_id,
        "packageId": document.package_id,
        "version": document.version,
        "signedAt": document.signed_at,
        "files": document.files,
    });
    let canonical = canonicalize(&payload)?;
    signature::UnparsedPublicKey::new(&signature::ED25519, key)
        .verify(canonical.as_bytes(), &signature)
        .map_err(|_| "Invalid package signature".to_owned())?;

    let actual = package_files(root)?;
    let files = payload
        .get("files")
        .and_then(Value::as_object)
        .ok_or_else(|| "signature.json is malformed".to_owned())?;
    if actual.len() != files.len() {
        return Err("Package contents do not match signature (file count mismatch)".to_owned());
    }
    for relative in actual {
        let expected = files
            .get(&relative)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("Unsigned file present in package: {relative}"))?;
        let bytes = fs::read(root.join(&relative))
            .map_err(|_| format!("Failed to hash package file: {relative}"))?;
        let actual = hex_lower(digest::digest(&digest::SHA256, &bytes).as_ref());
        if actual != expected {
            return Err(format!("Tampered file detected: {relative}"));
        }
    }
    Ok(())
}

fn package_files(root: &Path) -> Result<Vec<String>, String> {
    fn walk(root: &Path, current: &Path, output: &mut Vec<String>) -> Result<(), String> {
        let mut entries = fs::read_dir(current)
            .map_err(|_| "Failed to read package contents".to_owned())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "Failed to read package contents".to_owned())?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)
                .map_err(|_| "Failed to read package contents".to_owned())?;
            if metadata.file_type().is_symlink() {
                return Err("Failed to read package contents".to_owned());
            }
            if metadata.is_dir() {
                walk(root, &path, output)?;
            } else if metadata.is_file() {
                let relative = path
                    .strip_prefix(root)
                    .map_err(|_| "Failed to read package contents".to_owned())?
                    .to_string_lossy()
                    .replace('\\', "/");
                if relative != SIGNATURE_FILE {
                    output.push(relative);
                }
            } else {
                return Err("Failed to read package contents".to_owned());
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    walk(root, root, &mut files)?;
    files.sort();
    Ok(files)
}

fn canonicalize(value: &Value) -> Result<String, String> {
    match value {
        Value::Array(values) => {
            let entries = values
                .iter()
                .map(canonicalize)
                .collect::<Result<Vec<_>, _>>()?
                .join(",");
            Ok(format!("[{entries}]"))
        }
        Value::Object(object) => {
            let mut keys = object.keys().collect::<Vec<_>>();
            keys.sort();
            let entries = keys
                .into_iter()
                .map(|key| {
                    let encoded_key = serde_json::to_string(key)
                        .map_err(|error| format!("canonicalize JSON key {key:?}: {error}"))?;
                    let encoded_value = canonicalize(&object[key])?;
                    Ok(format!("{encoded_key}:{encoded_value}"))
                })
                .collect::<Result<Vec<_>, String>>()?
                .join(",");
            Ok(format!("{{{entries}}}"))
        }
        primitive => serde_json::to_string(primitive)
            .map_err(|error| format!("canonicalize JSON value: {error}")),
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn validate_path_id(value: &str, label: &str) -> Result<(), String> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.starts_with('.')
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(format!("Invalid {label}: {value:?}"));
    }
    Ok(())
}

fn assert_contained(base: &Path, candidate: &Path, label: &str) -> Result<(), String> {
    let base = normalize_lexical(base);
    let candidate = normalize_lexical(candidate);
    if candidate == base || candidate.starts_with(&base) {
        Ok(())
    } else {
        Err(format!(
            "{label} escapes package root: {}",
            candidate.display()
        ))
    }
}

fn normalize_lexical(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            other => result.push(other.as_os_str()),
        }
    }
    result
}

pub fn apply_platform_options(
    platform: &str,
    options: &[VppPlatformOption],
    selected: &BTreeMap<String, String>,
) -> String {
    if options.is_empty() {
        return platform.to_owned();
    }
    let mut effective = platform.to_owned();
    for option in options {
        let chosen = selected
            .get(&option.key)
            .filter(|value| {
                option
                    .values
                    .iter()
                    .any(|candidate| candidate.id == **value)
            })
            .cloned()
            .unwrap_or_else(|| option.default.clone());
        if !chosen.is_empty() {
            effective.push(':');
            effective.push_str(&option.key);
            effective.push('=');
            effective.push_str(&chosen);
        }
    }
    effective
}

pub fn runtime_v4_plugin_files(
    board: &InstalledVppBoard,
    vendor_screen_data: &Value,
    device_pins: &[Value],
) -> Result<BTreeMap<String, Vec<u8>>, String> {
    if board.device.target.target_type != "runtime-v4" {
        return Ok(BTreeMap::new());
    }
    let mut files = BTreeMap::new();
    let mut plugin_name = "vendor_plugin".to_owned();

    if let Some(relative) = board.device.hal.config_template.as_deref() {
        let template_path =
            resolve_package_relative(&board.package_path, relative, "hal.configTemplate")?;
        let raw = fs::read_to_string(&template_path)
            .map_err(|error| format!("read {}: {error}", template_path.display()))?;
        let template: Value = serde_json::from_str(&raw)
            .map_err(|error| format!("parse {}: {error}", template_path.display()))?;
        if let Some(name) = template.get("plugin_name").and_then(Value::as_str) {
            validate_path_id(name, "configTemplate.plugin_name")?;
            plugin_name = name.to_owned();
        }
        let config = generate_vendor_plugin_config(template, vendor_screen_data, device_pins);
        files.insert(
            format!("conf/{plugin_name}.json"),
            serde_json::to_vec_pretty(&config)
                .map_err(|error| format!("serialize VPP plugin config: {error}"))?,
        );
        files.insert(
            "vpp_plugins.conf".to_owned(),
            format!(
                "{plugin_name},./build/vpp/lib{plugin_name}_plugin.so,1,1,./build/vpp/{plugin_name}.json,\n"
            )
            .into_bytes(),
        );
    }

    let Some(plugin_entry) = board.device.hal.plugin_entry.as_deref() else {
        return Ok(files);
    };
    let relative_dir = if board.device.hal.provisioning.as_deref() == Some("prebuilt") {
        PathBuf::from(plugin_entry)
    } else {
        Path::new(plugin_entry)
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .to_path_buf()
    };
    let plugin_dir =
        resolve_package_relative_path(&board.package_path, &relative_dir, "hal.pluginEntry")?;
    if !plugin_dir.is_dir() {
        return Err(format!(
            "VPP plugin source path is not a directory: {}",
            plugin_dir.display()
        ));
    }

    let mut copied = Vec::<(String, Vec<u8>)>::new();
    collect_plugin_files(&plugin_dir, &plugin_dir, &mut copied)?;
    if copied.is_empty() {
        return Ok(files);
    }
    copied.sort_by(|left, right| left.0.cmp(&right.0));
    let mut checksum_lines = Vec::new();
    for (relative, content) in &copied {
        let hash = hex_lower(digest::digest(&digest::SHA256, content).as_ref());
        checksum_lines.extend_from_slice(format!("{hash}  {relative}\n").as_bytes());
        files.insert(format!("vpp_plugin/{relative}"), content.clone());
    }
    let combined = hex_lower(digest::digest(&digest::SHA256, &checksum_lines).as_ref());
    files.insert(
        "vpp_plugin/checksum.sha256".to_owned(),
        format!("{combined}\n").into_bytes(),
    );
    Ok(files)
}

fn resolve_package_relative(root: &Path, relative: &str, label: &str) -> Result<PathBuf, String> {
    resolve_package_relative_path(root, Path::new(relative), label)
}

fn resolve_package_relative_path(
    root: &Path,
    relative: &Path,
    label: &str,
) -> Result<PathBuf, String> {
    if relative.is_absolute() {
        return Err(format!("{label} must be package-relative"));
    }
    let candidate = root.join(relative);
    assert_contained(root, &candidate, label)?;
    Ok(candidate)
}

fn read_package_json(root: &Path, relative: &str, label: &str) -> Result<Value, String> {
    const MAX_JSON_BYTES: u64 = 8 * 1024 * 1024;
    let path = resolve_package_relative(root, relative, label)?;
    let metadata = fs::symlink_metadata(&path)
        .map_err(|error| format!("inspect {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!(
            "{label} is not a regular package file: {}",
            path.display()
        ));
    }
    if metadata.len() > MAX_JSON_BYTES {
        return Err(format!(
            "{label} exceeds {MAX_JSON_BYTES} bytes: {}",
            path.display()
        ));
    }
    let raw =
        fs::read_to_string(&path).map_err(|error| format!("read {}: {error}", path.display()))?;
    serde_json::from_str(&raw).map_err(|error| format!("parse {}: {error}", path.display()))
}

fn collect_plugin_files(
    root: &Path,
    current: &Path,
    output: &mut Vec<(String, Vec<u8>)>,
) -> Result<(), String> {
    let mut entries = fs::read_dir(current)
        .map_err(|error| format!("read {}: {error}", current.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("read {}: {error}", current.display()))?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        if matches!(
            entry.file_name().to_str(),
            Some("config_template.json" | "requirements.txt")
        ) {
            continue;
        }
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("inspect {}: {error}", path.display()))?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            collect_plugin_files(root, &path, output)?;
            continue;
        }
        if !metadata.is_file() {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|_| format!("plugin file escapes source root: {}", path.display()))?
            .to_string_lossy()
            .replace('\\', "/");
        output.push((
            relative,
            fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))?,
        ));
    }
    Ok(())
}

fn generate_vendor_plugin_config(
    mut template: Value,
    vendor_screen_data: &Value,
    device_pins: &[Value],
) -> Value {
    let Some(result) = template.as_object_mut() else {
        return template;
    };
    if let Some(data) = vendor_screen_data.as_object() {
        for (key, value) in data {
            if matches!(key.as_str(), "module-configuration" | "io-mapping") {
                continue;
            }
            if let Some(object) = value.as_object() {
                for (field, value) in object {
                    result.insert(field.clone(), value.clone());
                }
            }
        }
        result.insert("slots".to_owned(), build_runtime_slots(data));
    } else {
        result.insert("slots".to_owned(), Value::Array(Vec::new()));
    }
    let pins = build_runtime_pins(device_pins);
    if !pins.is_empty() {
        result.insert("pins".to_owned(), Value::Array(pins));
    }
    template
}

fn build_runtime_pins(device_pins: &[Value]) -> Vec<Value> {
    let mut pins = Vec::new();
    for pin in device_pins {
        let Some(pin_number) = pin
            .get("pin")
            .and_then(Value::as_str)
            .and_then(|value| value.parse::<u64>().ok())
        else {
            continue;
        };
        let pin_type = pin
            .get("pinType")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let address = pin
            .get("address")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match pin_type {
            "digitalInput" | "digitalOutput" => {
                let Some((byte, bit)) = parse_bit_address(address) else {
                    continue;
                };
                pins.push(serde_json::json!({
                    "pin": pin_number,
                    "direction": if pin_type == "digitalInput" { "input" } else { "output" },
                    "byte": byte,
                    "bit": bit
                }));
            }
            "analogOutput" => {
                let Some(word) = parse_word_address(address) else {
                    continue;
                };
                pins.push(serde_json::json!({"pin":pin_number,"direction":"pwm","word":word}));
            }
            _ => {}
        }
    }
    pins
}

fn build_runtime_slots(data: &serde_json::Map<String, Value>) -> Value {
    let assignments = data
        .get("module-configuration")
        .and_then(|value| value.get("slots"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let io_entries = data
        .get("io-mapping")
        .and_then(|value| value.get("entries"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut slots = Vec::new();
    for (index, module_id) in assignments.iter().enumerate() {
        let Some(module_id) = module_id.as_str() else {
            continue;
        };
        let slot_number = index + 1;
        let mut ranges = serde_json::Map::new();
        let mut di = Vec::new();
        let mut dout = Vec::new();
        let mut ai = Vec::new();
        let mut ao = Vec::new();
        let mut aid = Vec::new();
        let mut aod = Vec::new();
        for entry in &io_entries {
            if entry.get("slot").and_then(Value::as_u64) != Some(slot_number as u64) {
                continue;
            }
            let address = entry
                .get("iecAddress")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if address.starts_with("%IX") {
                di.push(address.to_owned());
            } else if address.starts_with("%QX") {
                dout.push(address.to_owned());
            } else if address.starts_with("%IW") {
                ai.push(address.to_owned());
            } else if address.starts_with("%QW") {
                ao.push(address.to_owned());
            } else if address.starts_with("%ID") {
                aid.push(address.to_owned());
            } else if address.starts_with("%QD") {
                aod.push(address.to_owned());
            }
        }
        if let Some(value) = bit_range(&di) {
            ranges.insert("digital_inputs".to_owned(), value);
        }
        if let Some(value) = bit_range(&dout) {
            ranges.insert("digital_outputs".to_owned(), value);
        }
        if let Some(value) = word_range(&ai, "base_word") {
            ranges.insert("analog_inputs".to_owned(), value);
        }
        if let Some(value) = word_range(&ao, "base_word") {
            ranges.insert("analog_outputs".to_owned(), value);
        }
        if let Some(value) = dword_range(&aid) {
            ranges.insert("analog_real_inputs".to_owned(), value);
        }
        if let Some(value) = dword_range(&aod) {
            ranges.insert("analog_real_outputs".to_owned(), value);
        }
        slots.push(serde_json::json!({
            "slot": slot_number,
            "module_id": module_id,
            "io_mapping": ranges
        }));
    }
    Value::Array(slots)
}

fn parse_bit_address(address: &str) -> Option<(u64, u64)> {
    let rest = address
        .strip_prefix("%IX")
        .or_else(|| address.strip_prefix("%QX"))?;
    let (byte, bit) = rest.split_once('.')?;
    Some((byte.parse().ok()?, bit.parse().ok()?))
}

fn parse_word_address(address: &str) -> Option<u64> {
    address
        .strip_prefix("%IW")
        .or_else(|| address.strip_prefix("%QW"))?
        .parse()
        .ok()
}

fn parse_dword_address(address: &str) -> Option<u64> {
    address
        .strip_prefix("%ID")
        .or_else(|| address.strip_prefix("%QD"))?
        .parse()
        .ok()
}

fn bit_range(addresses: &[String]) -> Option<Value> {
    let mut values = addresses
        .iter()
        .filter_map(|address| parse_bit_address(address))
        .collect::<Vec<_>>();
    values.sort_by_key(|(byte, bit)| byte * 8 + bit);
    let (byte, bit) = *values.first()?;
    Some(serde_json::json!({"base_byte":byte,"base_bit":bit,"count":values.len()}))
}

fn word_range(addresses: &[String], base_key: &str) -> Option<Value> {
    let mut values = addresses
        .iter()
        .filter_map(|address| parse_word_address(address))
        .collect::<Vec<_>>();
    values.sort_unstable();
    let first = *values.first()?;
    Some(serde_json::json!({base_key:first,"count":values.len()}))
}

fn dword_range(addresses: &[String]) -> Option<Value> {
    let mut values = addresses
        .iter()
        .filter_map(|address| parse_dword_address(address))
        .collect::<Vec<_>>();
    values.sort_unstable();
    let first = *values.first()?;
    Some(serde_json::json!({"base_dword":first,"count":values.len()}))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;

    use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
    use base64::Engine;
    use ring::rand::SystemRandom;
    use ring::signature::{Ed25519KeyPair, KeyPair};
    use tempfile::tempdir;

    use super::{
        apply_platform_options, canonicalize, hex_lower, verify_package_signature,
        VppPackageManager, VppPlatformOption, VppPlatformOptionValue,
    };

    #[test]
    fn platform_options_match_pinned_fqbn_shape() {
        let option = VppPlatformOption {
            key: "cpu".to_owned(),
            label: "Processor".to_owned(),
            default: "atmega328".to_owned(),
            help: None,
            values: vec![
                VppPlatformOptionValue {
                    id: "atmega328".to_owned(),
                    label: "ATmega328P".to_owned(),
                    help: None,
                },
                VppPlatformOptionValue {
                    id: "atmega328old".to_owned(),
                    label: "ATmega328P (Old Bootloader)".to_owned(),
                    help: None,
                },
            ],
        };
        assert_eq!(
            apply_platform_options(
                "arduino:avr:nano",
                std::slice::from_ref(&option),
                &BTreeMap::new()
            ),
            "arduino:avr:nano:cpu=atmega328"
        );
        let selected = BTreeMap::from([("cpu".to_owned(), "atmega328old".to_owned())]);
        assert_eq!(
            apply_platform_options("arduino:avr:nano", &[option], &selected),
            "arduino:avr:nano:cpu=atmega328old"
        );
    }

    #[test]
    fn verifies_signed_package_and_rejects_tampering() {
        let temp = tempdir().unwrap();
        fs::write(
            temp.path().join("manifest.json"),
            "{\"formatVersion\":\"1.0\"}",
        )
        .unwrap();
        fs::create_dir(temp.path().join("hal")).unwrap();
        fs::write(temp.path().join("hal/hal.cpp"), "void hardwareInit() {}\n").unwrap();

        let files = BTreeMap::from([
            (
                "hal/hal.cpp".to_owned(),
                hex_lower(
                    ring::digest::digest(&ring::digest::SHA256, b"void hardwareInit() {}\n")
                        .as_ref(),
                ),
            ),
            (
                "manifest.json".to_owned(),
                hex_lower(
                    ring::digest::digest(&ring::digest::SHA256, b"{\"formatVersion\":\"1.0\"}")
                        .as_ref(),
                ),
            ),
        ]);
        let payload = serde_json::json!({
            "formatVersion":"1.0","alg":"ed25519","keyId":"test-key",
            "packageId":"com.test.pkg","version":"1.0.0","signedAt":"2026-06-01T00:00:00.000Z",
            "files":files
        });
        let rng = SystemRandom::new();
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
        let keypair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
        let signature = keypair.sign(canonicalize(&payload).unwrap().as_bytes());
        let mut document = payload.as_object().unwrap().clone();
        document.insert(
            "signature".to_owned(),
            serde_json::Value::String(BASE64_STANDARD.encode(signature.as_ref())),
        );
        fs::write(
            temp.path().join("signature.json"),
            serde_json::to_vec_pretty(&document).unwrap(),
        )
        .unwrap();
        let trusted = BTreeMap::from([(
            "test-key".to_owned(),
            keypair.public_key().as_ref().to_vec(),
        )]);
        verify_package_signature(temp.path(), &trusted).unwrap();
        fs::write(temp.path().join("hal/hal.cpp"), "tampered").unwrap();
        assert!(verify_package_signature(temp.path(), &trusted)
            .unwrap_err()
            .contains("Tampered"));
    }

    #[test]
    fn manager_rejects_unsigned_packages() {
        let temp = tempdir().unwrap();
        let manager = VppPackageManager::with_packages_dir_and_keys(
            temp.path().join("packages"),
            BTreeMap::new(),
        );
        let bogus = temp.path().join("bogus.vpp");
        fs::write(&bogus, b"not a zip").unwrap();
        let error = manager.import_from_file(&bogus).unwrap_err();
        assert!(error.contains("Invalid VPP ZIP"));
    }
}
