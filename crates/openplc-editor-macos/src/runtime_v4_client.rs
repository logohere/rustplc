use std::io::Write;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ureq::tls::TlsConfig;
use ureq::Agent;
use uuid::Uuid;

const RUNTIME_API_PORT: u16 = 8443;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const UPLOAD_TIMEOUT: Duration = Duration::from_secs(300);
const COMPILE_TIMEOUT: Duration = Duration::from_secs(20 * 60);
const COMPILE_POLL_INTERVAL: Duration = Duration::from_secs(1);
const COMPILE_MAX_CONSECUTIVE_ERRORS: usize = 10;
const START_TIMEOUT: Duration = Duration::from_secs(5);
const START_POLL_INTERVAL: Duration = Duration::from_millis(150);
const MAX_RUNTIME_RESPONSE_BYTES: u64 = 16 * 1024 * 1024;
pub const MIN_STRUCPP_RUNTIME_VERSION: &str = "4.1.0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsersInfo {
    pub has_users: bool,
    pub runtime_version: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeUserRole {
    Admin,
    User,
}

impl RuntimeUserRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::User => "user",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeUser {
    pub id: u64,
    pub username: String,
    pub role: RuntimeUserRole,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpdateUserParams {
    pub username: Option<String>,
    pub password: Option<String>,
    pub current_password: Option<String>,
    pub role: Option<RuntimeUserRole>,
}

pub const MIN_USER_MANAGEMENT_RUNTIME_VERSION: &str = "4.1.9";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedRuntimeVersion {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub prerelease: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RuntimeStatus {
    pub status: String,
    #[serde(default)]
    pub timing_stats: Option<Value>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct CompilationStatus {
    pub status: String,
    #[serde(default)]
    pub logs: Vec<String>,
    #[serde(default)]
    pub exit_code: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeLogLevel {
    Info,
    Warning,
    Error,
    Debug,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompilationOutcome {
    Success,
    Failed,
    Timeout,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartOutcome {
    Started,
    Failed,
    Timeout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeployOutcome {
    Started,
    CompileFailed,
    CompileTimeout,
    CompileError,
    StartFailed,
    StartTimeout,
}

#[derive(Debug, Clone, Default)]
struct Session {
    token: Option<String>,
}

#[derive(Clone)]
pub struct RuntimeV4Client {
    base_url: String,
    agent: Agent,
    upload_agent: Agent,
    session: Arc<Mutex<Session>>,
}

impl RuntimeV4Client {
    pub fn new_with_tls(ip_address: &str, allow_insecure_tls: bool) -> Result<Self, String> {
        let ip_address = ip_address.trim();
        if ip_address.is_empty() {
            return Err("Runtime IP address is empty.".to_owned());
        }
        Self::with_base_url(
            format!("https://{ip_address}:{RUNTIME_API_PORT}"),
            allow_insecure_tls,
        )
    }

    fn with_base_url(base_url: String, allow_insecure_tls: bool) -> Result<Self, String> {
        let tls = TlsConfig::builder()
            .disable_verification(allow_insecure_tls)
            .build();
        let agent: Agent = Agent::config_builder()
            .timeout_global(Some(REQUEST_TIMEOUT))
            .http_status_as_error(false)
            .tls_config(tls.clone())
            .build()
            .into();
        let upload_agent: Agent = Agent::config_builder()
            .timeout_global(Some(UPLOAD_TIMEOUT))
            .http_status_as_error(false)
            .tls_config(tls)
            .build()
            .into();
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_owned(),
            agent,
            upload_agent,
            session: Arc::new(Mutex::new(Session::default())),
        })
    }

    #[cfg(test)]
    fn for_test(base_url: String) -> Self {
        Self::with_base_url(base_url, false).unwrap()
    }

    pub fn users_info(&self) -> Result<UsersInfo, String> {
        let response = self.get_raw("/api/get-users-info", None)?;
        Ok(match response.status {
            200 => UsersInfo {
                has_users: true,
                runtime_version: response.runtime_version,
            },
            404 => UsersInfo {
                has_users: false,
                runtime_version: response.runtime_version,
            },
            status => {
                return Err(http_error(status, &response.body));
            }
        })
    }

    pub fn runtime_version(&self) -> Result<String, String> {
        let response = self.get_raw("/api/version", None)?;
        ensure_success(&response)?;
        let value: Value = serde_json::from_str(&response.body)
            .map_err(|error| format!("parse runtime version response: {error}"))?;
        value
            .get("version")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| "Runtime version response did not contain a string version.".to_owned())
    }

    pub fn create_first_user(&self, username: &str, password: &str) -> Result<(), String> {
        let body = serde_json::to_vec(&serde_json::json!({
            "username": username,
            "password": password,
        }))
        .map_err(|error| format!("serialize create-user request: {error}"))?;
        let response = self.post_raw(
            "/api/create-user",
            Some("application/json"),
            &body,
            None,
            false,
        )?;
        if response.status == 201 {
            Ok(())
        } else {
            Err(http_error(response.status, &response.body))
        }
    }

    pub fn list_users(&self) -> Result<Vec<RuntimeUser>, String> {
        let response = self.authenticated_get("/api/get-users-info")?;
        ensure_success(&response)?;
        let value: Value = serde_json::from_str(&response.body)
            .map_err(|error| format!("parse runtime users response: {error}"))?;
        if !value.is_array() {
            return Ok(Vec::new());
        }
        serde_json::from_value(value).map_err(|error| format!("parse runtime users: {error}"))
    }

    pub fn who_am_i(&self) -> Result<RuntimeUser, String> {
        let response = self.authenticated_get("/api/whoami")?;
        ensure_success(&response)?;
        serde_json::from_str(&response.body)
            .map_err(|error| format!("parse runtime whoami response: {error}"))
    }

    pub fn create_user(
        &self,
        username: &str,
        password: &str,
        role: RuntimeUserRole,
    ) -> Result<(), String> {
        let body = serde_json::to_vec(&serde_json::json!({
            "username": username,
            "password": password,
            "role": role.as_str(),
        }))
        .map_err(|error| format!("serialize create-user request: {error}"))?;
        let response =
            self.authenticated_post("/api/create-user", Some("application/json"), &body, false)?;
        if response.status == 201 {
            Ok(())
        } else {
            Err(http_error(response.status, &response.body))
        }
    }

    pub fn update_user(&self, user_id: u64, params: &UpdateUserParams) -> Result<(), String> {
        let mut body = serde_json::Map::new();
        if let Some(username) = params.username.as_ref() {
            body.insert("username".to_owned(), Value::String(username.clone()));
        }
        if let Some(password) = params.password.as_ref() {
            body.insert("password".to_owned(), Value::String(password.clone()));
        }
        if let Some(current_password) = params.current_password.as_ref() {
            body.insert(
                "current_password".to_owned(),
                Value::String(current_password.clone()),
            );
        }
        if let Some(role) = params.role.as_ref() {
            body.insert("role".to_owned(), Value::String(role.as_str().to_owned()));
        }
        let payload = serde_json::to_vec(&body)
            .map_err(|error| format!("serialize update-user request: {error}"))?;
        let response = self.authenticated_put(
            &format!("/api/update-user/{user_id}"),
            Some("application/json"),
            &payload,
        )?;
        ensure_success(&response)
    }

    pub fn delete_user(&self, user_id: u64) -> Result<(), String> {
        let response = self.authenticated_delete(&format!("/api/delete-user/{user_id}"))?;
        ensure_success(&response)
    }

    pub fn login(&self, username: &str, password: &str) -> Result<(), String> {
        let token = self.authenticate(username, password)?;
        let mut session = self
            .session
            .lock()
            .map_err(|_| "runtime session lock poisoned".to_owned())?;
        session.token = Some(token);
        Ok(())
    }

    pub fn logout(&self) -> Result<(), String> {
        *self
            .session
            .lock()
            .map_err(|_| "runtime session lock poisoned".to_owned())? = Session::default();
        Ok(())
    }

    pub fn status(&self, include_stats: bool) -> Result<RuntimeStatus, String> {
        let path = if include_stats {
            "/api/status?include_stats=true"
        } else {
            "/api/status"
        };
        let response = self.authenticated_get(path)?;
        ensure_success(&response)?;
        serde_json::from_str(&response.body)
            .map_err(|error| format!("parse runtime status response: {error}"))
    }

    pub fn stop_plc(&self) -> Result<(), String> {
        let response = self.authenticated_get("/api/stop-plc")?;
        ensure_success(&response).map(|_| ())
    }

    pub fn start_plc(&self) -> Result<String, String> {
        let response = self.authenticated_get("/api/start-plc")?;
        ensure_success(&response)?;
        let value: Value = serde_json::from_str(&response.body)
            .map_err(|error| format!("parse start PLC response: {error}"))?;
        Ok(value
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned())
    }

    pub fn compilation_status(&self) -> Result<CompilationStatus, String> {
        let response = self.authenticated_get("/api/compilation-status")?;
        ensure_success(&response)?;
        serde_json::from_str(&response.body)
            .map_err(|error| format!("parse compilation status response: {error}"))
    }

    pub fn upload_file(
        &self,
        bytes: &[u8],
        filename: &str,
        file_content_type: &str,
        clean_build: bool,
    ) -> Result<String, String> {
        let path = if clean_build {
            "/api/upload-file?clean=1"
        } else {
            "/api/upload-file"
        };
        let boundary = format!("----WebKitFormBoundary{}", Uuid::new_v4().simple());
        let mut body = Vec::with_capacity(bytes.len() + 256);
        write!(
            body,
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: {file_content_type}\r\n\r\n"
        )
        .map_err(|error| format!("compose upload header: {error}"))?;
        body.extend_from_slice(bytes);
        write!(body, "\r\n--{boundary}--\r\n")
            .map_err(|error| format!("compose upload footer: {error}"))?;
        let content_type = format!("multipart/form-data; boundary={boundary}");
        let response = self.authenticated_post(path, Some(&content_type), &body, true)?;
        if response.status == 200 {
            Ok(response.body)
        } else {
            Err(http_error(response.status, &response.body))
        }
    }

    pub fn deploy_file(
        &self,
        bytes: &[u8],
        filename: &str,
        file_content_type: &str,
        clean_build: bool,
        mut on_log: impl FnMut(RuntimeLogLevel, &str),
    ) -> Result<DeployOutcome, String> {
        self.upload_file(bytes, filename, file_content_type, clean_build)?;
        let compile = poll_compilation_with(
            || self.compilation_status(),
            &mut on_log,
            COMPILE_TIMEOUT,
            COMPILE_POLL_INTERVAL,
            COMPILE_MAX_CONSECUTIVE_ERRORS,
        );
        match compile {
            CompilationOutcome::Success => {}
            CompilationOutcome::Failed => return Ok(DeployOutcome::CompileFailed),
            CompilationOutcome::Timeout => return Ok(DeployOutcome::CompileTimeout),
            CompilationOutcome::Error => return Ok(DeployOutcome::CompileError),
        }
        let start = start_after_build_with(
            || self.start_plc(),
            &mut on_log,
            START_TIMEOUT,
            START_POLL_INTERVAL,
        );
        Ok(match start {
            StartOutcome::Started => DeployOutcome::Started,
            StartOutcome::Failed => DeployOutcome::StartFailed,
            StartOutcome::Timeout => DeployOutcome::StartTimeout,
        })
    }

    pub fn deploy_program(
        &self,
        zip: &[u8],
        clean_build: bool,
        on_log: impl FnMut(RuntimeLogLevel, &str),
    ) -> Result<DeployOutcome, String> {
        self.deploy_file(zip, "program.zip", "application/zip", clean_build, on_log)
    }

    fn authenticate(&self, username: &str, password: &str) -> Result<String, String> {
        let body = serde_json::to_vec(&LoginRequest { username, password })
            .map_err(|error| format!("serialize login request: {error}"))?;
        let response = self.post_raw("/api/login", Some("application/json"), &body, None, false)?;
        if response.status != 200 {
            return Err(http_error(response.status, &response.body));
        }
        let login: LoginResponse = serde_json::from_str(&response.body)
            .map_err(|error| format!("parse login response: {error}"))?;
        if login.access_token.trim().is_empty() {
            return Err("Runtime returned an empty access token.".to_owned());
        }
        Ok(login.access_token)
    }

    fn authenticated_get(&self, path: &str) -> Result<RawResponse, String> {
        let token = self.current_token()?;
        let first = self.get_raw(path, Some(&token))?;
        if token_expired_status(first.status) {
            self.expire_session()?;
            return Err("Runtime session expired. Log in again to continue.".to_owned());
        }
        Ok(first)
    }

    fn authenticated_post(
        &self,
        path: &str,
        content_type: Option<&str>,
        body: &[u8],
        upload: bool,
    ) -> Result<RawResponse, String> {
        let token = self.current_token()?;
        let first = self.post_raw(path, content_type, body, Some(&token), upload)?;
        if token_expired_status(first.status) {
            self.expire_session()?;
            return Err("Runtime session expired. Log in again to continue.".to_owned());
        }
        Ok(first)
    }

    fn authenticated_put(
        &self,
        path: &str,
        content_type: Option<&str>,
        body: &[u8],
    ) -> Result<RawResponse, String> {
        let token = self.current_token()?;
        let first = self.put_raw(path, content_type, body, Some(&token))?;
        if token_expired_status(first.status) {
            self.expire_session()?;
            return Err("Runtime session expired. Log in again to continue.".to_owned());
        }
        Ok(first)
    }

    fn authenticated_delete(&self, path: &str) -> Result<RawResponse, String> {
        let token = self.current_token()?;
        let first = self.delete_raw(path, Some(&token))?;
        if token_expired_status(first.status) {
            self.expire_session()?;
            return Err("Runtime session expired. Log in again to continue.".to_owned());
        }
        Ok(first)
    }

    fn expire_session(&self) -> Result<(), String> {
        self.session
            .lock()
            .map_err(|_| "runtime session lock poisoned".to_owned())?
            .token = None;
        Ok(())
    }

    pub fn access_token(&self) -> Result<String, String> {
        self.current_token()
    }

    fn current_token(&self) -> Result<String, String> {
        self.session
            .lock()
            .map_err(|_| "runtime session lock poisoned".to_owned())?
            .token
            .clone()
            .ok_or_else(|| "Login to the Runtime before using this action.".to_owned())
    }

    fn get_raw(&self, path: &str, token: Option<&str>) -> Result<RawResponse, String> {
        let url = format!("{}{path}", self.base_url);
        let mut request = self.agent.get(&url);
        if let Some(token) = token {
            request = request.header("Authorization", &format!("Bearer {token}"));
        }
        let response = request.call().map_err(|error| error.to_string())?;
        raw_response(response)
    }

    fn post_raw(
        &self,
        path: &str,
        content_type: Option<&str>,
        body: &[u8],
        token: Option<&str>,
        upload: bool,
    ) -> Result<RawResponse, String> {
        let url = format!("{}{path}", self.base_url);
        let agent = if upload {
            &self.upload_agent
        } else {
            &self.agent
        };
        let mut request = agent.post(&url);
        if let Some(content_type) = content_type {
            request = request.header("Content-Type", content_type);
        }
        if let Some(token) = token {
            request = request.header("Authorization", &format!("Bearer {token}"));
        }
        let response = request.send(body).map_err(|error| error.to_string())?;
        raw_response(response)
    }

    fn put_raw(
        &self,
        path: &str,
        content_type: Option<&str>,
        body: &[u8],
        token: Option<&str>,
    ) -> Result<RawResponse, String> {
        let url = format!("{}{path}", self.base_url);
        let mut request = self.agent.put(&url);
        if let Some(content_type) = content_type {
            request = request.header("Content-Type", content_type);
        }
        if let Some(token) = token {
            request = request.header("Authorization", &format!("Bearer {token}"));
        }
        let response = request.send(body).map_err(|error| error.to_string())?;
        raw_response(response)
    }

    fn delete_raw(&self, path: &str, token: Option<&str>) -> Result<RawResponse, String> {
        let url = format!("{}{path}", self.base_url);
        let mut request = self.agent.delete(&url);
        if let Some(token) = token {
            request = request.header("Authorization", &format!("Bearer {token}"));
        }
        let response = request.call().map_err(|error| error.to_string())?;
        raw_response(response)
    }
}

#[derive(Serialize)]
struct LoginRequest<'a> {
    username: &'a str,
    password: &'a str,
}

#[derive(Deserialize)]
struct LoginResponse {
    access_token: String,
}

#[derive(Debug)]
struct RawResponse {
    status: u16,
    body: String,
    runtime_version: Option<String>,
}

fn raw_response(mut response: ureq::http::Response<ureq::Body>) -> Result<RawResponse, String> {
    let status = response.status().as_u16();
    let runtime_version = response
        .headers()
        .get("x-openplc-runtime-version")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let body = response
        .body_mut()
        .with_config()
        .limit(MAX_RUNTIME_RESPONSE_BYTES)
        .read_to_string()
        .map_err(|error| {
            format!("read runtime response (max {MAX_RUNTIME_RESPONSE_BYTES} bytes): {error}")
        })?;
    Ok(RawResponse {
        status,
        body,
        runtime_version,
    })
}

fn ensure_success(response: &RawResponse) -> Result<(), String> {
    if (200..300).contains(&response.status) {
        Ok(())
    } else {
        Err(http_error(response.status, &response.body))
    }
}

fn http_error(status: u16, body: &str) -> String {
    let body = body.trim();
    if body.is_empty() {
        format!("Runtime HTTP {status}")
    } else {
        let truncated = body.chars().take(1024).collect::<String>();
        let suffix = if truncated.len() < body.len() {
            "…"
        } else {
            ""
        };
        format!("Runtime HTTP {status}: {truncated}{suffix}")
    }
}

pub fn parse_runtime_version(raw: Option<&str>) -> Option<ParsedRuntimeVersion> {
    let raw = raw?.trim();
    if raw.is_empty() {
        return None;
    }
    let raw = raw.strip_prefix('v').unwrap_or(raw);
    let (core, prerelease) = raw
        .split_once('-')
        .map(|(core, prerelease)| (core, Some(prerelease.to_owned())))
        .unwrap_or((raw, None));
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(ParsedRuntimeVersion {
        major,
        minor,
        patch,
        prerelease,
    })
}

pub fn is_strucpp_compatible_runtime(raw: Option<&str>) -> bool {
    let Some(version) = parse_runtime_version(raw) else {
        return false;
    };
    if version.major > 4 {
        return true;
    }
    version.major == 4 && version.minor >= 1
}

pub fn is_user_management_capable_runtime(raw: Option<&str>) -> bool {
    let Some(version) = parse_runtime_version(raw) else {
        return false;
    };
    if version.major != 4 {
        return version.major > 4;
    }
    if version.minor != 1 {
        return version.minor > 1;
    }
    version.patch >= 9
}

pub fn describe_incompatible_runtime(raw: Option<&str>) -> String {
    let reported = raw
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("unknown");
    format!(
        "Runtime version {reported} is not compatible with this editor. Upload requires OpenPLC Runtime v{MIN_STRUCPP_RUNTIME_VERSION} or newer (STruC++ pipeline). Please upgrade the runtime on the target device before pushing this build."
    )
}

fn token_expired_status(status: u16) -> bool {
    matches!(status, 401 | 403)
}

fn classify_log_line(line: &str) -> (RuntimeLogLevel, String) {
    let trimmed = line.trim_end_matches(['\r', '\n']);
    let upper = trimmed.to_ascii_uppercase();
    for (prefix, level) in [
        ("[ERROR]", RuntimeLogLevel::Error),
        ("[WARN]", RuntimeLogLevel::Warning),
        ("[WARNING]", RuntimeLogLevel::Warning),
        ("[DEBUG]", RuntimeLogLevel::Debug),
        ("[INFO]", RuntimeLogLevel::Info),
    ] {
        if upper.starts_with(prefix) {
            return (level, trimmed[prefix.len()..].trim_start().to_owned());
        }
    }
    (RuntimeLogLevel::Info, trimmed.to_owned())
}

fn poll_compilation_with(
    mut fetch: impl FnMut() -> Result<CompilationStatus, String>,
    on_log: &mut impl FnMut(RuntimeLogLevel, &str),
    timeout: Duration,
    interval: Duration,
    max_consecutive_errors: usize,
) -> CompilationOutcome {
    let deadline = Instant::now() + timeout;
    let mut last_log_count = 0usize;
    let mut consecutive_errors = 0usize;
    loop {
        if Instant::now() > deadline {
            on_log(
                RuntimeLogLevel::Error,
                "Compilation status polling timed out.",
            );
            return CompilationOutcome::Timeout;
        }
        if !interval.is_zero() {
            thread::sleep(interval);
        }
        let status = match fetch() {
            Ok(status) => {
                consecutive_errors = 0;
                status
            }
            Err(error) => {
                consecutive_errors += 1;
                if consecutive_errors >= max_consecutive_errors {
                    on_log(
                        RuntimeLogLevel::Error,
                        &format!(
                            "Error polling compilation status: {error} ({max_consecutive_errors} consecutive failures, giving up)"
                        ),
                    );
                    return CompilationOutcome::Error;
                }
                continue;
            }
        };
        for raw in status.logs.iter().skip(last_log_count) {
            let (level, message) = classify_log_line(raw);
            on_log(level, &message);
        }
        last_log_count = status.logs.len();
        match status.status.as_str() {
            "SUCCESS" => {
                on_log(
                    RuntimeLogLevel::Info,
                    &format!(
                        "Compilation completed successfully (exit code: {}).",
                        status.exit_code.unwrap_or(0)
                    ),
                );
                return CompilationOutcome::Success;
            }
            "FAILED" => {
                on_log(
                    RuntimeLogLevel::Error,
                    &format!(
                        "Compilation failed (exit code: {}).",
                        status.exit_code.unwrap_or(1)
                    ),
                );
                return CompilationOutcome::Failed;
            }
            _ => {}
        }
    }
}

fn start_after_build_with(
    mut fetch: impl FnMut() -> Result<String, String>,
    on_log: &mut impl FnMut(RuntimeLogLevel, &str),
    timeout: Duration,
    interval: Duration,
) -> StartOutcome {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let status = match fetch() {
            Ok(status) => status,
            Err(error) => {
                on_log(
                    RuntimeLogLevel::Error,
                    &format!("Failed to start PLC: {error}"),
                );
                return StartOutcome::Failed;
            }
        };
        if status.contains("START:OK") || status.contains("ALREADY_RUNNING") {
            on_log(RuntimeLogLevel::Info, "PLC started.");
            return StartOutcome::Started;
        }
        if !status.contains("BUSY") {
            on_log(
                RuntimeLogLevel::Error,
                &format!(
                    "Failed to start PLC: {}",
                    if status.is_empty() {
                        "unknown response"
                    } else {
                        &status
                    }
                ),
            );
            return StartOutcome::Failed;
        }
        if !interval.is_zero() {
            thread::sleep(interval);
        }
    }
    on_log(
        RuntimeLogLevel::Warning,
        "PLC did not start before the runtime busy deadline. Press Play to retry.",
    );
    StartOutcome::Timeout
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::{Arc, Mutex};

    use super::{
        classify_log_line, describe_incompatible_runtime, is_strucpp_compatible_runtime,
        parse_runtime_version, poll_compilation_with, start_after_build_with, CompilationOutcome,
        CompilationStatus, RuntimeLogLevel, RuntimeUserRole, RuntimeV4Client, StartOutcome,
        UpdateUserParams,
    };
    use std::time::Duration;

    #[test]
    fn runtime_version_gate_matches_the_pinned_strucpp_line() {
        assert!(parse_runtime_version(Some("v4")).is_none());
        assert!(parse_runtime_version(Some("dev")).is_none());
        let rc = parse_runtime_version(Some("v4.1.0-rc.3")).unwrap();
        assert_eq!((rc.major, rc.minor, rc.patch), (4, 1, 0));
        assert_eq!(rc.prerelease.as_deref(), Some("rc.3"));
        assert!(!is_strucpp_compatible_runtime(Some("4.0.99")));
        assert!(is_strucpp_compatible_runtime(Some("4.1.0-rc.3")));
        assert!(is_strucpp_compatible_runtime(Some("4.1.0")));
        assert!(is_strucpp_compatible_runtime(Some("5.0.0")));
        assert!(!is_strucpp_compatible_runtime(None));
        let message = describe_incompatible_runtime(Some("v4"));
        assert!(message.contains("Runtime version v4"));
        assert!(message.contains("v4.1.0 or newer"));
    }

    #[test]
    fn unauthenticated_runtime_version_probe_uses_api_version() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let observed = Arc::new(Mutex::new(String::new()));
        let observed_server = observed.clone();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let request = read_request(stream.try_clone().unwrap());
            *observed_server.lock().unwrap() = request;
            write_response(stream, &http_response(200, "{\"version\":\"v4.1.9\"}"));
        });
        let client = RuntimeV4Client::for_test(format!("http://{address}"));
        assert_eq!(client.runtime_version().unwrap(), "v4.1.9");
        server.join().unwrap();
        assert!(observed
            .lock()
            .unwrap()
            .starts_with("GET /api/version HTTP/1.1"));
    }

    #[test]
    fn log_classification_matches_pinned_prefix_behavior() {
        assert_eq!(
            classify_log_line("[ERROR] compiler failed\n"),
            (RuntimeLogLevel::Error, "compiler failed".to_owned())
        );
        assert_eq!(
            classify_log_line("[WARN] slow\n"),
            (RuntimeLogLevel::Warning, "slow".to_owned())
        );
        assert_eq!(
            classify_log_line("plain\n"),
            (RuntimeLogLevel::Info, "plain".to_owned())
        );
    }

    #[test]
    fn compilation_poll_streams_only_new_logs_and_stops_on_success() {
        let mut scripted = vec![
            CompilationStatus {
                status: "COMPILING".to_owned(),
                logs: vec!["[INFO] one\n".to_owned()],
                exit_code: None,
            },
            CompilationStatus {
                status: "SUCCESS".to_owned(),
                logs: vec!["[INFO] one\n".to_owned(), "[WARN] two\n".to_owned()],
                exit_code: Some(0),
            },
        ]
        .into_iter();
        let logs = Arc::new(Mutex::new(Vec::new()));
        let sink = logs.clone();
        let outcome = poll_compilation_with(
            || Ok(scripted.next().unwrap()),
            &mut |level, message| sink.lock().unwrap().push((level, message.to_owned())),
            Duration::from_secs(1),
            Duration::ZERO,
            3,
        );
        assert_eq!(outcome, CompilationOutcome::Success);
        let logs = logs.lock().unwrap();
        assert_eq!(
            logs.iter().filter(|(_, message)| message == "one").count(),
            1
        );
        assert!(logs
            .iter()
            .any(|(level, message)| *level == RuntimeLogLevel::Warning && message == "two"));
    }

    #[test]
    fn start_after_build_retries_busy_then_accepts_start_ok() {
        let mut statuses = vec!["COMMAND:BUSY", "COMMAND:BUSY", "START:OK"].into_iter();
        let outcome = start_after_build_with(
            || Ok(statuses.next().unwrap().to_owned()),
            &mut |_, _| {},
            Duration::from_secs(1),
            Duration::ZERO,
        );
        assert_eq!(outcome, StartOutcome::Started);
    }

    #[test]
    fn multipart_upload_matches_the_pinned_runtime_v3_contract() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let observed = Arc::new(Mutex::new(Vec::<String>::new()));
        let observed_server = observed.clone();
        let server = std::thread::spawn(move || {
            for step in 0..2 {
                let (stream, _) = listener.accept().unwrap();
                let request = read_request(stream.try_clone().unwrap());
                observed_server.lock().unwrap().push(request);
                let response = if step == 0 {
                    http_response(200, "{\"access_token\":\"token\"}")
                } else {
                    http_response(200, "{\"CompilationStatus\":\"COMPILING\"}")
                };
                write_response(stream, &response);
            }
        });

        let client = RuntimeV4Client::for_test(format!("http://{address}"));
        client.login("admin", "secret").unwrap();
        client
            .upload_file(
                b"PROGRAM main\nEND_PROGRAM\n",
                "program.st",
                "text/plain",
                true,
            )
            .unwrap();
        server.join().unwrap();
        let request = observed.lock().unwrap()[1].clone();
        let lower = request.to_ascii_lowercase();
        assert!(request.starts_with("POST /api/upload-file?clean=1 HTTP/1.1"));
        assert!(lower.contains("authorization: bearer token"));
        assert!(lower.contains("content-type: multipart/form-data; boundary="));
        assert!(request.contains("filename=\"program.st\""));
        assert!(request.contains("Content-Type: text/plain"));
        assert!(request.contains("PROGRAM main\nEND_PROGRAM"));
    }

    #[test]
    fn user_management_protocol_matches_pinned_runtime_paths_and_payloads() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let observed = Arc::new(Mutex::new(Vec::<String>::new()));
        let observed_server = observed.clone();
        let server = std::thread::spawn(move || {
            let responses = [
                http_response(200, "{\"access_token\":\"token\"}"),
                http_response(200, "[{\"id\":1,\"username\":\"admin\",\"role\":\"admin\"},{\"id\":2,\"username\":\"tech\",\"role\":\"user\"}]"),
                http_response(200, "{\"id\":1,\"username\":\"admin\",\"role\":\"admin\"}"),
                http_response(201, "{}"),
                http_response(200, "{}"),
                http_response(200, "{}"),
            ];
            for response in responses {
                let (stream, _) = listener.accept().unwrap();
                let request = read_request(stream.try_clone().unwrap());
                observed_server.lock().unwrap().push(request);
                write_response(stream, &response);
            }
        });

        let client = RuntimeV4Client::for_test(format!("http://{address}"));
        client.login("admin", "secret").unwrap();
        let users = client.list_users().unwrap();
        assert_eq!(users.len(), 2);
        assert_eq!(client.who_am_i().unwrap().username, "admin");
        client
            .create_user("operator", "pw", RuntimeUserRole::User)
            .unwrap();
        client
            .update_user(
                2,
                &UpdateUserParams {
                    username: Some("technician".to_owned()),
                    password: Some("newpw".to_owned()),
                    current_password: None,
                    role: Some(RuntimeUserRole::Admin),
                },
            )
            .unwrap();
        client.delete_user(2).unwrap();
        server.join().unwrap();

        let requests = observed.lock().unwrap();
        assert!(requests[1].starts_with("GET /api/get-users-info HTTP/1.1"));
        assert!(requests[2].starts_with("GET /api/whoami HTTP/1.1"));
        assert!(requests[3].starts_with("POST /api/create-user HTTP/1.1"));
        assert!(requests[3].contains("\"role\":\"user\""));
        assert!(requests[4].starts_with("PUT /api/update-user/2 HTTP/1.1"));
        assert!(requests[4].contains("\"username\":\"technician\""));
        assert!(requests[4].contains("\"role\":\"admin\""));
        assert!(requests[5].starts_with("DELETE /api/delete-user/2 HTTP/1.1"));
        for request in requests.iter().skip(1) {
            assert!(request
                .to_ascii_lowercase()
                .contains("authorization: bearer token"));
        }
    }

    #[test]
    fn user_management_version_gate_matches_pinned_419_boundary() {
        assert!(!super::is_user_management_capable_runtime(Some("v4.1.8")));
        assert!(super::is_user_management_capable_runtime(Some("v4.1.9")));
        assert!(super::is_user_management_capable_runtime(Some(
            "4.1.9-rc.1"
        )));
        assert!(super::is_user_management_capable_runtime(Some("v4.2.0")));
        assert!(super::is_user_management_capable_runtime(Some("v5.0.0")));
        assert!(!super::is_user_management_capable_runtime(Some("v3.9.9")));
    }

    #[test]
    fn expired_status_token_is_cleared_and_requires_explicit_login() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let observed = Arc::new(Mutex::new(Vec::<String>::new()));
        let observed_server = observed.clone();
        let server = std::thread::spawn(move || {
            for step in 0..2 {
                let (stream, _) = listener.accept().unwrap();
                let request = read_request(stream.try_clone().unwrap());
                observed_server.lock().unwrap().push(request);
                let response = if step == 0 {
                    http_response(200, "{\"access_token\":\"token-1\"}")
                } else {
                    http_response(401, "expired")
                };
                write_response(stream, &response);
            }
        });

        let client = RuntimeV4Client::for_test(format!("http://{address}"));
        client.login("admin", "secret").unwrap();
        let error = client.status(false).unwrap_err();
        assert!(error.contains("session expired"));
        assert!(client
            .access_token()
            .unwrap_err()
            .contains("Login to the Runtime"));
        server.join().unwrap();
        let observed = observed.lock().unwrap();
        assert_eq!(observed.len(), 2);
        assert!(observed[1]
            .to_ascii_lowercase()
            .contains("authorization: bearer token-1"));
    }

    fn read_request(stream: TcpStream) -> String {
        let mut reader = BufReader::new(stream);
        let mut output = String::new();
        let mut content_length = 0usize;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" || line.is_empty() {
                break;
            }
            if line.to_ascii_lowercase().starts_with("content-length:") {
                content_length = line
                    .split_once(':')
                    .and_then(|(_, value)| value.trim().parse().ok())
                    .unwrap_or(0);
            }
            output.push_str(&line);
        }
        if content_length > 0 {
            let mut body = vec![0; content_length];
            reader.read_exact(&mut body).unwrap();
            output.push_str(&String::from_utf8_lossy(&body));
        }
        output
    }

    fn http_response(status: u16, body: &str) -> Vec<u8> {
        let reason = match status {
            200 => "OK",
            401 => "Unauthorized",
            _ => "Error",
        };
        format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .into_bytes()
    }

    fn write_response(mut stream: TcpStream, response: &[u8]) {
        stream.write_all(response).unwrap();
        stream.flush().unwrap();
    }
}
