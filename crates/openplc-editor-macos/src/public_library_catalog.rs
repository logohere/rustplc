use std::time::Duration;

use serde::Deserialize;
use ureq::Agent;

const DEFAULT_EDGE_API_URL: &str = "https://api.autonomylogic.com";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const CATALOG_LIMIT: usize = 50;
pub const MAX_PUBLIC_LIBRARY_BYTES: u64 = 64 * 1024 * 1024;
const MAX_CATALOG_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PublicLibrary {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub version: String,
    pub display_name: String,
    pub description: Option<String>,
    pub license: Option<String>,
    pub author_handle: String,
    pub manifest_pous: ManifestPous,
    pub size_bytes: u64,
    pub sha256: String,
    pub downloads_count: u64,
    pub published_at: String,
    pub updated_at: String,
    pub is_project_public: bool,
    pub project_url: Option<String>,
    pub project_stars_count: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManifestPous {
    pub functions: Vec<String>,
    pub function_blocks: Vec<String>,
    pub types: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PublicLibraryPage {
    pub libraries: Vec<PublicLibrary>,
    pub total: usize,
    pub page: usize,
    pub limit: usize,
    pub total_pages: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Envelope<T> {
    status_code: u16,
    data: T,
}

pub fn list_public_libraries(query: &str) -> Result<PublicLibraryPage, String> {
    let base = edge_base_url();
    let url = format!("{base}/public/libraries");
    let agent: Agent = Agent::config_builder()
        .timeout_global(Some(REQUEST_TIMEOUT))
        .http_status_as_error(false)
        .build()
        .into();
    let mut request = agent
        .get(&url)
        .query("page", "1")
        .query("limit", CATALOG_LIMIT.to_string())
        .query("sort", "popular");
    let query = query.trim();
    if !query.is_empty() {
        request = request.query("q", query);
    }
    let response = request
        .call()
        .map_err(|error| format!("Public library catalog request failed: {error}"))?;
    read_catalog_page(response)
}

pub fn download_public_library(id: &str) -> Result<String, String> {
    validate_catalog_id(id)?;
    let base = edge_base_url();
    let url = format!("{base}/public/libraries/{id}/download");
    let agent: Agent = Agent::config_builder()
        .timeout_global(Some(REQUEST_TIMEOUT))
        .http_status_as_error(false)
        .build()
        .into();
    let response = agent
        .get(&url)
        .call()
        .map_err(|error| format!("Public library download failed: {error}"))?;
    read_text_response(response, "Public library download")
}

fn edge_base_url() -> String {
    let candidate = std::env::var("OPENPLC_EDGE_API_URL")
        .ok()
        .map(|value| value.trim().trim_end_matches('/').to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_EDGE_API_URL.to_owned());
    if catalog_base_url_allowed(&candidate) {
        candidate
    } else {
        DEFAULT_EDGE_API_URL.to_owned()
    }
}

fn catalog_base_url_allowed(value: &str) -> bool {
    if value.starts_with("https://") {
        return true;
    }
    let Some(rest) = value.strip_prefix("http://") else {
        return false;
    };
    let authority = rest.split('/').next().unwrap_or_default();
    let host = authority
        .rsplit_once(':')
        .map(|(host, port)| {
            if port.bytes().all(|byte| byte.is_ascii_digit()) {
                host
            } else {
                authority
            }
        })
        .unwrap_or(authority)
        .trim_matches(['[', ']']);
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}

fn read_catalog_page(
    mut response: ureq::http::Response<ureq::Body>,
) -> Result<PublicLibraryPage, String> {
    let status = response.status().as_u16();
    let body = response
        .body_mut()
        .with_config()
        .limit(MAX_CATALOG_RESPONSE_BYTES)
        .read_to_string()
        .map_err(|error| format!("read public library catalog response (max {MAX_CATALOG_RESPONSE_BYTES} bytes): {error}"))?;
    if !(200..300).contains(&status) {
        return Err(http_error("Public library catalog", status, &body));
    }
    parse_catalog_page(&body)
}

fn read_text_response(
    mut response: ureq::http::Response<ureq::Body>,
    label: &str,
) -> Result<String, String> {
    let status = response.status().as_u16();
    let body = response
        .body_mut()
        .with_config()
        .limit(MAX_PUBLIC_LIBRARY_BYTES)
        .read_to_string()
        .map_err(|error| {
            format!("read {label} response (max {MAX_PUBLIC_LIBRARY_BYTES} bytes): {error}")
        })?;
    if (200..300).contains(&status) {
        Ok(body)
    } else {
        Err(http_error(label, status, &body))
    }
}

fn parse_catalog_page(body: &str) -> Result<PublicLibraryPage, String> {
    if let Ok(envelope) = serde_json::from_str::<Envelope<PublicLibraryPage>>(body) {
        if !(200..300).contains(&envelope.status_code) {
            return Err(format!(
                "Public library catalog response statusCode was {}",
                envelope.status_code
            ));
        }
        return validate_page(envelope.data);
    }
    let page = serde_json::from_str::<PublicLibraryPage>(body)
        .map_err(|error| format!("parse public library catalog response: {error}"))?;
    validate_page(page)
}

fn validate_page(page: PublicLibraryPage) -> Result<PublicLibraryPage, String> {
    if page.limit > CATALOG_LIMIT {
        return Err(format!(
            "Public library catalog returned an invalid page limit {}",
            page.limit
        ));
    }
    for library in &page.libraries {
        validate_catalog_id(&library.id)?;
        if library.name.trim().is_empty()
            || library.version.trim().is_empty()
            || library.display_name.trim().is_empty()
            || library.author_handle.trim().is_empty()
            || library.size_bytes > MAX_PUBLIC_LIBRARY_BYTES
            || library.sha256.len() != 64
            || !library.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(format!(
                "Public library catalog returned invalid metadata for {:?}",
                library.id
            ));
        }
    }
    Ok(page)
}

fn validate_catalog_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(format!("Invalid public library id {id:?}."));
    }
    Ok(())
}

fn http_error(label: &str, status: u16, body: &str) -> String {
    let body = body.trim();
    if body.is_empty() {
        format!("{label} HTTP {status}")
    } else {
        let truncated = body.chars().take(200).collect::<String>();
        format!("{label} HTTP {status}: {truncated}")
    }
}

#[cfg(test)]
mod tests {
    use super::{
        catalog_base_url_allowed, parse_catalog_page, validate_catalog_id, MAX_PUBLIC_LIBRARY_BYTES,
    };

    const PAGE: &str = r#"{
      "statusCode": 200,
      "data": {
        "libraries": [{
          "id":"lib_123","projectId":"project_1","name":"demo-lib","version":"1.2.3",
          "displayName":"Demo Library","description":"demo","license":"MIT","authorHandle":"alice",
          "manifestPous":{"functions":["ADD"],"functionBlocks":[],"types":[]},
          "sizeBytes":1234,"sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
          "downloadsCount":7,"publishedAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-01T00:00:00Z",
          "isProjectPublic":true,"projectUrl":null,"projectStarsCount":2
        }],
        "total":1,"page":1,"limit":50,"totalPages":1
      }
    }"#;

    #[test]
    fn parses_the_pinned_public_catalog_envelope() {
        let page = parse_catalog_page(PAGE).unwrap();
        assert_eq!(page.libraries[0].name, "demo-lib");
        assert_eq!(page.libraries[0].manifest_pous.functions, ["ADD"]);
        assert_eq!(page.limit, 50);
    }

    #[test]
    fn catalog_base_url_rejects_plaintext_non_loopback_hosts() {
        assert!(catalog_base_url_allowed("https://api.autonomylogic.com"));
        assert!(catalog_base_url_allowed("http://127.0.0.1:3000"));
        assert!(catalog_base_url_allowed("http://localhost:3000"));
        assert!(catalog_base_url_allowed("http://[::1]:3000"));
        assert!(!catalog_base_url_allowed("http://192.168.1.20:3000"));
        assert!(!catalog_base_url_allowed("file:///tmp/catalog"));
    }

    #[test]
    fn catalog_ids_are_bounded_and_path_safe() {
        assert!(validate_catalog_id("abc-123_DEF").is_ok());
        assert!(validate_catalog_id("../escape").is_err());
        assert!(validate_catalog_id("slash/name").is_err());
    }
    #[test]
    fn catalog_rejects_entries_larger_than_the_library_input_ceiling() {
        let oversized = PAGE.replace(
            "\"sizeBytes\":1234",
            &format!("\"sizeBytes\":{}", MAX_PUBLIC_LIBRARY_BYTES + 1),
        );
        assert!(parse_catalog_page(&oversized).is_err());
    }
}
