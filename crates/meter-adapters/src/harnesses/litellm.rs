use std::fmt::{Debug, Formatter};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use meter_core::{
    EventPayload, HarnessKind, LitellmConfig, MeterEvent, ModelName, RunMetrics, TokenUsage,
};
use meter_engine::{
    EventSender, HarnessAdapter, HarnessCapabilities, HarnessError, HarnessRunRequest,
    HarnessRunResult,
};
use serde_json::{Value, json};
use thiserror::Error;

pub const LITELLM_API_KEY_ENV: &str = "LITELLM_API_KEY";
pub const LITELLM_API_BASE_ENV: &str = "LITELLM_API_BASE";
const DEFAULT_LITELLM_API_BASE: &str = "https://api.litellm.ai";
const DEFAULT_LITELLM_MODEL: &str = "gpt-5";

#[derive(Clone, PartialEq, Eq)]
pub struct LitellmApiKey(String);

impl LitellmApiKey {
    pub fn from_env() -> Result<Self, LitellmCredentialsError> {
        Self::from_optional_value(std::env::var(LITELLM_API_KEY_ENV).ok())
    }

    pub fn from_optional_value(value: Option<String>) -> Result<Self, LitellmCredentialsError> {
        let Some(value) = value else {
            return Err(LitellmCredentialsError::Missing);
        };
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(LitellmCredentialsError::Missing);
        }
        Ok(Self(trimmed.to_owned()))
    }

    fn expose_secret(&self) -> &str {
        &self.0
    }
}

impl Debug for LitellmApiKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("LitellmApiKey(REDACTED)")
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum LitellmCredentialsError {
    #[error("LiteLLM API key is required; set LITELLM_API_KEY before using --harness litellm")]
    Missing,
}

#[derive(Clone)]
pub enum LitellmCredentialSource {
    Env,
    Value(Option<String>),
}

impl Debug for LitellmCredentialSource {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Env => f.write_str("LitellmCredentialSource::Env"),
            Self::Value(Some(_)) => f.write_str("LitellmCredentialSource::Value(REDACTED)"),
            Self::Value(None) => f.write_str("LitellmCredentialSource::Value(None)"),
        }
    }
}

impl LitellmCredentialSource {
    fn load(&self) -> Result<LitellmApiKey, LitellmCredentialsError> {
        match self {
            Self::Env => LitellmApiKey::from_env(),
            Self::Value(value) => LitellmApiKey::from_optional_value(value.clone()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LitellmAdapter {
    config: LitellmConfig,
    credentials: LitellmCredentialSource,
    client: Arc<dyn LitellmApiClient>,
}

impl LitellmAdapter {
    pub fn new(config: LitellmConfig) -> Self {
        Self {
            config,
            credentials: LitellmCredentialSource::Env,
            client: Arc::new(HttpLitellmApiClient::from_env()),
        }
    }

    pub fn with_client(
        config: LitellmConfig,
        credentials: LitellmCredentialSource,
        client: Arc<dyn LitellmApiClient>,
    ) -> Self {
        Self {
            config,
            credentials,
            client,
        }
    }
}

#[async_trait]
impl HarnessAdapter for LitellmAdapter {
    fn kind(&self) -> HarnessKind {
        HarnessKind::Litellm
    }

    fn capabilities(&self) -> HarnessCapabilities {
        HarnessCapabilities {
            supports_resume: false,
            supports_workspace: false,
            supports_event_stream: false,
            reports_token_usage: true,
            reports_model: true,
        }
    }

    async fn run(
        &self,
        request: HarnessRunRequest,
        events: EventSender,
    ) -> Result<HarnessRunResult, HarnessError> {
        let api_key = self
            .credentials
            .load()
            .map_err(|error| HarnessError::UnsupportedConfig(error.to_string()))?;
        let model = request
            .model
            .clone()
            .or_else(|| self.config.model.clone())
            .unwrap_or_else(default_model);
        let api_request = LitellmChatRequest {
            model: model.clone(),
            prompt: request.prompt,
        };
        let response = self
            .client
            .chat_completion(&api_key, api_request)
            .await
            .map_err(|error| HarnessError::Api(error.to_string()))?;

        if let Some(content) = response
            .content
            .as_deref()
            .filter(|content| !content.is_empty())
        {
            println!("{content}");
        }
        let resolved_model = response.model.or(Some(model));
        let metrics = RunMetrics {
            provider_event_count: 1,
            turn_count: 1,
            token_usage: response.usage.unwrap_or_default(),
            ..RunMetrics::default()
        };
        if metrics.token_usage != TokenUsage::default() {
            events
                .send(MeterEvent::new(
                    request.context.with_resolved_model(resolved_model.clone()),
                    EventPayload::UsageReported(metrics.token_usage.clone()),
                ))
                .await
                .map_err(|_| {
                    HarnessError::Io(std::io::Error::new(
                        std::io::ErrorKind::BrokenPipe,
                        "event writer closed",
                    ))
                })?;
        }

        Ok(HarnessRunResult {
            success: true,
            session_id: request.session_id,
            resolved_model,
            metrics,
            exit_code: Some(0),
            failure_reason: None,
        })
    }
}

fn default_model() -> ModelName {
    ModelName::new(DEFAULT_LITELLM_MODEL).unwrap_or_else(|err| panic!("{err}"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LitellmChatRequest {
    pub model: ModelName,
    pub prompt: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LitellmChatResponse {
    pub model: Option<ModelName>,
    pub content: Option<String>,
    pub usage: Option<TokenUsage>,
}

#[async_trait]
pub trait LitellmApiClient: Debug + Send + Sync {
    async fn chat_completion(
        &self,
        api_key: &LitellmApiKey,
        request: LitellmChatRequest,
    ) -> Result<LitellmChatResponse, LitellmApiError>;
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum LitellmApiError {
    #[error("LiteLLM API request failed: {0}")]
    Request(String),
    #[error("LiteLLM API returned HTTP {status}: {message}")]
    Http { status: u16, message: String },
    #[error("LiteLLM API response was invalid: {0}")]
    InvalidResponse(String),
}

#[derive(Debug, Clone)]
pub struct HttpLitellmApiClient {
    base_url: String,
    timeout: Duration,
}

impl HttpLitellmApiClient {
    pub fn from_env() -> Self {
        let base_url = std::env::var(LITELLM_API_BASE_ENV)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_LITELLM_API_BASE.to_owned());
        Self {
            base_url,
            timeout: Duration::from_secs(60),
        }
    }

    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            timeout: Duration::from_secs(60),
        }
    }
}

#[async_trait]
impl LitellmApiClient for HttpLitellmApiClient {
    async fn chat_completion(
        &self,
        api_key: &LitellmApiKey,
        request: LitellmChatRequest,
    ) -> Result<LitellmChatResponse, LitellmApiError> {
        let endpoint = chat_completions_endpoint(&self.base_url);
        let timeout = self.timeout;
        let api_key = api_key.expose_secret().to_owned();
        tokio::task::spawn_blocking(move || send_api_request(&endpoint, &api_key, request, timeout))
            .await
            .map_err(|_| LitellmApiError::Request("request task was interrupted".to_owned()))?
    }
}

fn chat_completions_endpoint(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.ends_with("/chat/completions") {
        trimmed.to_owned()
    } else if trimmed.ends_with("/v1") {
        format!("{trimmed}/chat/completions")
    } else {
        format!("{trimmed}/v1/chat/completions")
    }
}

fn send_api_request(
    endpoint: &str,
    api_key: &str,
    request: LitellmChatRequest,
    timeout: Duration,
) -> Result<LitellmChatResponse, LitellmApiError> {
    if endpoint.starts_with("https://") {
        send_curl_request(endpoint, api_key, request, timeout)
    } else {
        send_plain_http_request(endpoint, api_key, request, timeout)
    }
}

fn send_plain_http_request(
    endpoint: &str,
    api_key: &str,
    request: LitellmChatRequest,
    timeout: Duration,
) -> Result<LitellmChatResponse, LitellmApiError> {
    let parsed = ParsedHttpUrl::parse(endpoint)?;
    let body = request_body(request)?;
    let mut stream = TcpStream::connect((parsed.host.as_str(), parsed.port))
        .map_err(|error| LitellmApiError::Request(error.to_string()))?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|error| LitellmApiError::Request(error.to_string()))?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|error| LitellmApiError::Request(error.to_string()))?;
    let http_request = format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nAccept: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        parsed.path,
        parsed.host,
        api_key,
        body.len()
    );
    stream
        .write_all(http_request.as_bytes())
        .and_then(|_| stream.write_all(&body))
        .map_err(|error| LitellmApiError::Request(error.to_string()))?;
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .map_err(|error| LitellmApiError::Request(error.to_string()))?;
    parse_http_response(&response)
}

fn send_curl_request(
    endpoint: &str,
    api_key: &str,
    request: LitellmChatRequest,
    timeout: Duration,
) -> Result<LitellmChatResponse, LitellmApiError> {
    let body = request_body(request)?;
    let body_path = write_temp_body(&body)?;
    let mut child = Command::new("curl")
        .arg("--silent")
        .arg("--show-error")
        .arg("--include")
        .arg("--request")
        .arg("POST")
        .arg("--url")
        .arg(endpoint)
        .arg("--max-time")
        .arg(timeout.as_secs().max(1).to_string())
        .arg("--data-binary")
        .arg(format!("@{}", body_path.display()))
        .arg("--config")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            let _ = std::fs::remove_file(&body_path);
            LitellmApiError::Request(format!(
                "failed to execute curl for LiteLLM API request: {error}"
            ))
        })?;
    let mut stdin = child.stdin.take().ok_or_else(|| {
        let _ = std::fs::remove_file(&body_path);
        LitellmApiError::Request("failed to open curl stdin".to_owned())
    })?;
    stdin
        .write_all(
            format!(
                "header = \"Authorization: Bearer {}\"\nheader = \"Content-Type: application/json\"\nheader = \"Accept: application/json\"\n",
                escape_curl_config(api_key)
            )
            .as_bytes(),
        )
        .map_err(|error| {
            let _ = std::fs::remove_file(&body_path);
            LitellmApiError::Request(error.to_string())
        })?;
    drop(stdin);
    let output = child.wait_with_output().map_err(|error| {
        let _ = std::fs::remove_file(&body_path);
        LitellmApiError::Request(error.to_string())
    })?;
    let _ = std::fs::remove_file(&body_path);
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = stderr
            .lines()
            .last()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .unwrap_or("curl request failed");
        return Err(LitellmApiError::Request(message.to_owned()));
    }
    parse_http_response(&output.stdout)
}

fn request_body(request: LitellmChatRequest) -> Result<Vec<u8>, LitellmApiError> {
    serde_json::to_vec(&json!({
        "model": request.model.as_str(),
        "messages": [
            {
                "role": "user",
                "content": request.prompt,
            }
        ],
    }))
    .map_err(|error| LitellmApiError::Request(error.to_string()))
}

fn write_temp_body(body: &[u8]) -> Result<std::path::PathBuf, LitellmApiError> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let path = std::env::temp_dir().join(format!(
        "svdo-meter-litellm-{}-{nanos}.json",
        std::process::id()
    ));
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;

        let mut options = std::fs::OpenOptions::new();
        options.create_new(true).write(true).mode(0o600);
        let mut file = options
            .open(&path)
            .map_err(|error| LitellmApiError::Request(error.to_string()))?;
        file.write_all(body)
            .map_err(|error| LitellmApiError::Request(error.to_string()))?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(&path, body).map_err(|error| LitellmApiError::Request(error.to_string()))?;
    }
    Ok(path)
}

fn escape_curl_config(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedHttpUrl {
    host: String,
    port: u16,
    path: String,
}

impl ParsedHttpUrl {
    fn parse(url: &str) -> Result<Self, LitellmApiError> {
        let without_scheme = url.strip_prefix("http://").ok_or_else(|| {
            LitellmApiError::Request("LiteLLM API URL must use http:// or https://".to_owned())
        })?;
        let (authority, path) = without_scheme
            .split_once('/')
            .unwrap_or((without_scheme, ""));
        let (host, port) = match authority.rsplit_once(':') {
            Some((host, port)) => {
                let port = port.parse::<u16>().map_err(|_| {
                    LitellmApiError::Request("LiteLLM API URL port is invalid".to_owned())
                })?;
                (host, port)
            }
            None => (authority, 80),
        };
        if host.is_empty() {
            return Err(LitellmApiError::Request(
                "LiteLLM API URL host is required".to_owned(),
            ));
        }
        Ok(Self {
            host: host.to_owned(),
            port,
            path: format!("/{}", path.trim_start_matches('/')),
        })
    }
}

fn parse_http_response(response: &[u8]) -> Result<LitellmChatResponse, LitellmApiError> {
    let response = String::from_utf8_lossy(response);
    let (head, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| LitellmApiError::InvalidResponse("missing HTTP response body".to_owned()))?;
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|status| status.parse::<u16>().ok())
        .ok_or_else(|| LitellmApiError::InvalidResponse("missing HTTP status".to_owned()))?;
    if !(200..300).contains(&status) {
        return Err(LitellmApiError::Http {
            status,
            message: extract_error_message(body),
        });
    }
    parse_chat_response(body)
}

fn parse_chat_response(body: &str) -> Result<LitellmChatResponse, LitellmApiError> {
    let value: Value = serde_json::from_str(body)
        .map_err(|error| LitellmApiError::InvalidResponse(error.to_string()))?;
    let model = value
        .get("model")
        .and_then(Value::as_str)
        .map(ModelName::new)
        .transpose()
        .map_err(|error| LitellmApiError::InvalidResponse(error.to_string()))?;
    let content = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let usage = value.get("usage").map(parse_usage).transpose()?;
    Ok(LitellmChatResponse {
        model,
        content,
        usage,
    })
}

fn parse_usage(value: &Value) -> Result<TokenUsage, LitellmApiError> {
    Ok(TokenUsage {
        input_tokens: optional_u64(value, "prompt_tokens")?,
        cached_input_tokens: value
            .get("prompt_tokens_details")
            .and_then(|details| details.get("cached_tokens"))
            .and_then(Value::as_u64),
        cache_write_tokens: None,
        output_tokens: optional_u64(value, "completion_tokens")?,
        reasoning_tokens: value
            .get("completion_tokens_details")
            .and_then(|details| details.get("reasoning_tokens"))
            .and_then(Value::as_u64),
    })
}

fn optional_u64(value: &Value, key: &str) -> Result<Option<u64>, LitellmApiError> {
    match value.get(key) {
        Some(value) => value.as_u64().map(Some).ok_or_else(|| {
            LitellmApiError::InvalidResponse(format!(
                "usage field `{key}` must be an unsigned integer"
            ))
        }),
        None => Ok(None),
    }
}

fn extract_error_message(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(|error| error.get("message").or_else(|| error.get("type")))
                .or_else(|| value.get("message"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .filter(|message| !message.trim().is_empty())
        .unwrap_or_else(|| "request was not accepted".to_owned())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use meter_core::{EventContext, EventPayload, EventType, RawEventRetention, RunId, TicketId};
    use meter_engine::{HarnessOptions, RunEngine, RunRequest};

    use super::*;
    use crate::JsonlEventStore;

    #[derive(Debug)]
    struct FakeLitellmClient {
        requests: Mutex<Vec<LitellmChatRequest>>,
        result: Mutex<Result<LitellmChatResponse, LitellmApiError>>,
    }

    impl Default for FakeLitellmClient {
        fn default() -> Self {
            Self {
                requests: Mutex::new(Vec::new()),
                result: Mutex::new(Ok(LitellmChatResponse::default())),
            }
        }
    }

    #[async_trait]
    impl LitellmApiClient for FakeLitellmClient {
        async fn chat_completion(
            &self,
            _api_key: &LitellmApiKey,
            request: LitellmChatRequest,
        ) -> Result<LitellmChatResponse, LitellmApiError> {
            self.requests
                .lock()
                .unwrap_or_else(|err| panic!("{err}"))
                .push(request);
            self.result
                .lock()
                .unwrap_or_else(|err| panic!("{err}"))
                .clone()
        }
    }

    #[test]
    fn litellm_api_key_rejects_missing_empty_and_whitespace_values() {
        for value in [None, Some(String::new()), Some("   ".to_owned())] {
            assert_eq!(
                LitellmApiKey::from_optional_value(value),
                Err(LitellmCredentialsError::Missing)
            );
        }
    }

    #[test]
    fn litellm_api_key_debug_redacts_secret() {
        let marker = "unit-test-litellm-key-redaction-marker";
        let key = LitellmApiKey::from_optional_value(Some(marker.to_owned()))
            .unwrap_or_else(|err| panic!("{err}"));
        let source = LitellmCredentialSource::Value(Some(marker.to_owned()));

        let debug = format!("{key:?}");
        let source_debug = format!("{source:?}");

        assert!(debug.contains("REDACTED"));
        assert!(source_debug.contains("REDACTED"));
        assert!(!debug.contains(marker));
        assert!(!source_debug.contains(marker));
    }

    #[tokio::test]
    async fn adapter_calls_litellm_api_and_maps_successful_response() {
        let model = ModelName::new("fixture-model").unwrap_or_else(|err| panic!("{err}"));
        let resolved = ModelName::new("resolved-model").unwrap_or_else(|err| panic!("{err}"));
        let client = Arc::new(FakeLitellmClient {
            result: Mutex::new(Ok(LitellmChatResponse {
                model: Some(resolved.clone()),
                content: Some("done".to_owned()),
                usage: Some(TokenUsage {
                    input_tokens: Some(11),
                    output_tokens: Some(7),
                    ..TokenUsage::default()
                }),
            })),
            ..FakeLitellmClient::default()
        });
        let adapter = LitellmAdapter::with_client(
            LitellmConfig { model: None },
            LitellmCredentialSource::Value(Some("unit-test-key".to_owned())),
            client.clone(),
        );
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);

        let result = adapter
            .run(run_request(Some(model.clone())), tx)
            .await
            .unwrap_or_else(|err| panic!("{err}"));

        assert!(result.success);
        assert_eq!(result.resolved_model, Some(resolved));
        assert_eq!(result.metrics.provider_event_count, 1);
        assert_eq!(result.metrics.turn_count, 1);
        assert_eq!(result.metrics.token_usage.input_tokens, Some(11));
        assert_eq!(result.metrics.token_usage.output_tokens, Some(7));
        assert_eq!(
            client
                .requests
                .lock()
                .unwrap_or_else(|err| panic!("{err}"))
                .as_slice(),
            &[LitellmChatRequest {
                model,
                prompt: "Do work".to_owned()
            }]
        );
        let usage_event = rx
            .recv()
            .await
            .unwrap_or_else(|| panic!("missing usage event"));
        assert_eq!(usage_event.event_type, meter_core::EventType::UsageReported);
    }

    #[tokio::test]
    async fn adapter_surfaces_api_failures_without_secret() {
        let marker = "unit-test-litellm-key-redaction-marker";
        let client = Arc::new(FakeLitellmClient {
            result: Mutex::new(Err(LitellmApiError::Http {
                status: 429,
                message: "rate limit exceeded".to_owned(),
            })),
            ..FakeLitellmClient::default()
        });
        let adapter = LitellmAdapter::with_client(
            LitellmConfig { model: None },
            LitellmCredentialSource::Value(Some(marker.to_owned())),
            client,
        );
        let (tx, _rx) = tokio::sync::mpsc::channel(8);

        let error = adapter
            .run(run_request(None), tx)
            .await
            .err()
            .unwrap_or_else(|| panic!("expected API error"));
        let error_debug = format!("{error:?}");
        let error_display = error.to_string();

        assert!(error_display.contains("LiteLLM API returned HTTP 429"));
        assert!(error_display.contains("rate limit exceeded"));
        assert!(!error_debug.contains(marker));
        assert!(!error_display.contains(marker));
    }

    #[tokio::test]
    async fn adapter_does_not_call_api_when_key_is_missing() {
        let client = Arc::new(FakeLitellmClient::default());
        let adapter = LitellmAdapter::with_client(
            LitellmConfig { model: None },
            LitellmCredentialSource::Value(None),
            client.clone(),
        );
        let (tx, _rx) = tokio::sync::mpsc::channel(8);

        let error = adapter
            .run(run_request(None), tx)
            .await
            .err()
            .unwrap_or_else(|| panic!("expected credential error"));
        let error_debug = format!("{error:?}");
        let error_display = error.to_string();

        assert!(error_display.contains(LITELLM_API_KEY_ENV));
        assert!(!error_debug.contains("Authorization"));
        assert!(!error_display.contains("Authorization"));
        assert!(
            client
                .requests
                .lock()
                .unwrap_or_else(|err| panic!("{err}"))
                .is_empty()
        );
    }

    #[tokio::test]
    async fn run_engine_writes_litellm_success_telemetry_without_api_key() {
        let marker = "unit-test-litellm-key-redaction-marker";
        let dir = tempfile::tempdir().unwrap_or_else(|err| panic!("{err}"));
        let store = Arc::new(JsonlEventStore::new(dir.path().join(".svdo").join("meter")));
        let resolved =
            ModelName::new("resolved-litellm-model").unwrap_or_else(|err| panic!("{err}"));
        let client = Arc::new(FakeLitellmClient {
            result: Mutex::new(Ok(LitellmChatResponse {
                model: Some(resolved.clone()),
                content: None,
                usage: Some(TokenUsage {
                    input_tokens: Some(13),
                    cached_input_tokens: Some(5),
                    output_tokens: Some(8),
                    reasoning_tokens: Some(2),
                    ..TokenUsage::default()
                }),
            })),
            ..FakeLitellmClient::default()
        });
        let adapter = Arc::new(LitellmAdapter::with_client(
            LitellmConfig { model: None },
            LitellmCredentialSource::Value(Some(marker.to_owned())),
            client,
        ));
        let engine = RunEngine::new(store).with_adapter(adapter);

        let outcome = engine
            .run(engine_run_request(dir.path(), "ENG-LITELLM-TELEMETRY"))
            .await
            .unwrap_or_else(|err| panic!("{err}"));

        assert!(outcome.success);
        let stream_path = dir
            .path()
            .join(".svdo")
            .join("meter")
            .join(format!("{}.jsonl", outcome.run_id));
        let telemetry = std::fs::read_to_string(&stream_path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", stream_path.display()));
        assert!(!telemetry.contains(marker));
        assert!(!telemetry.contains("Authorization"));

        let events = parse_events(&telemetry);
        assert_eq!(
            events
                .iter()
                .map(|event| event.event_type)
                .collect::<Vec<_>>(),
            vec![
                EventType::RunStarted,
                EventType::UsageReported,
                EventType::RunCompleted,
            ]
        );
        assert!(events.iter().all(|event| event.run_id == outcome.run_id));
        assert!(
            events
                .iter()
                .all(|event| event.harness == HarnessKind::Litellm)
        );
        assert!(
            events
                .iter()
                .all(|event| event.ticket_id.as_str() == "ENG-LITELLM-TELEMETRY")
        );
        assert_eq!(events[1].resolved_model, Some(resolved.clone()));
        let EventPayload::UsageReported(usage) = &events[1].payload else {
            panic!("expected usage telemetry");
        };
        assert_eq!(usage.input_tokens, Some(13));
        assert_eq!(usage.cached_input_tokens, Some(5));
        assert_eq!(usage.output_tokens, Some(8));
        assert_eq!(usage.reasoning_tokens, Some(2));
        let EventPayload::RunCompleted(completed) = &events[2].payload else {
            panic!("expected run completion telemetry");
        };
        assert_eq!(completed.metrics.provider_event_count, 1);
        assert_eq!(completed.metrics.turn_count, 1);
        assert_eq!(completed.metrics.token_usage.input_tokens, Some(13));
        assert_eq!(completed.metrics.token_usage.output_tokens, Some(8));
    }

    #[tokio::test]
    async fn run_engine_writes_litellm_api_failure_telemetry_without_api_key() {
        let marker = "unit-test-litellm-key-redaction-marker";
        let dir = tempfile::tempdir().unwrap_or_else(|err| panic!("{err}"));
        let store = Arc::new(JsonlEventStore::new(dir.path().join(".svdo").join("meter")));
        let client = Arc::new(FakeLitellmClient {
            result: Mutex::new(Err(LitellmApiError::Http {
                status: 503,
                message: "service unavailable".to_owned(),
            })),
            ..FakeLitellmClient::default()
        });
        let adapter = Arc::new(LitellmAdapter::with_client(
            LitellmConfig { model: None },
            LitellmCredentialSource::Value(Some(marker.to_owned())),
            client,
        ));
        let engine = RunEngine::new(store).with_adapter(adapter);

        let outcome = engine
            .run(engine_run_request(dir.path(), "ENG-LITELLM-FAIL"))
            .await
            .unwrap_or_else(|err| panic!("{err}"));

        assert!(!outcome.success);
        let failure_reason = outcome
            .failure_reason
            .as_deref()
            .unwrap_or_else(|| panic!("missing failure reason"));
        assert!(failure_reason.contains("LiteLLM API returned HTTP 503"));
        assert!(!failure_reason.contains(marker));
        assert!(!format!("{outcome:?}").contains(marker));

        let stream_path = dir
            .path()
            .join(".svdo")
            .join("meter")
            .join(format!("{}.jsonl", outcome.run_id));
        let telemetry = std::fs::read_to_string(&stream_path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", stream_path.display()));
        assert!(!telemetry.contains(marker));
        assert!(!telemetry.contains("Authorization"));

        let events = parse_events(&telemetry);
        assert_eq!(
            events
                .iter()
                .map(|event| event.event_type)
                .collect::<Vec<_>>(),
            vec![EventType::RunStarted, EventType::RunFailed]
        );
        assert!(events.iter().all(|event| event.run_id == outcome.run_id));
        assert!(
            events
                .iter()
                .all(|event| event.harness == HarnessKind::Litellm)
        );
        assert!(
            events
                .iter()
                .all(|event| event.ticket_id.as_str() == "ENG-LITELLM-FAIL")
        );
        let EventPayload::RunFailed(failed) = &events[1].payload else {
            panic!("expected run failure telemetry");
        };
        assert_eq!(failed.metrics.errors, 1);
        assert!(failed.reason.contains("LiteLLM API returned HTTP 503"));
        assert!(!failed.reason.contains(marker));
    }

    #[test]
    fn parses_litellm_chat_response_usage() {
        let response = parse_chat_response(
            r#"{
              "model": "gpt-fixture",
              "choices": [{"message": {"content": "ok"}}],
              "usage": {
                "prompt_tokens": 3,
                "completion_tokens": 5,
                "prompt_tokens_details": {"cached_tokens": 2},
                "completion_tokens_details": {"reasoning_tokens": 1}
              }
            }"#,
        )
        .unwrap_or_else(|err| panic!("{err}"));

        assert_eq!(
            response.model,
            Some(ModelName::new("gpt-fixture").unwrap_or_else(|err| panic!("{err}")))
        );
        assert_eq!(response.content, Some("ok".to_owned()));
        assert_eq!(
            response.usage.as_ref().and_then(|usage| usage.input_tokens),
            Some(3)
        );
        assert_eq!(
            response
                .usage
                .as_ref()
                .and_then(|usage| usage.output_tokens),
            Some(5)
        );
        assert_eq!(
            response
                .usage
                .as_ref()
                .and_then(|usage| usage.cached_input_tokens),
            Some(2)
        );
        assert_eq!(
            response
                .usage
                .as_ref()
                .and_then(|usage| usage.reasoning_tokens),
            Some(1)
        );
    }

    fn run_request(model: Option<ModelName>) -> HarnessRunRequest {
        HarnessRunRequest {
            context: EventContext {
                run_id: RunId::new(),
                ticket_id: TicketId::new("ENG-LITELLM").unwrap_or_else(|err| panic!("{err}")),
                label: None,
                harness: HarnessKind::Litellm,
                requested_model: model.clone(),
                resolved_model: None,
                session_id: None,
                workspace: None,
            },
            prompt: "Do work".to_owned(),
            session_id: None,
            model,
            raw_event_retention: RawEventRetention::Disabled,
            execution_permission: meter_core::ExecutionPermissionMode::Standard,
            options: HarnessOptions::empty(),
        }
    }

    fn engine_run_request(workspace: &std::path::Path, ticket: &str) -> RunRequest {
        RunRequest {
            ticket_id: TicketId::new(ticket).unwrap_or_else(|err| panic!("{err}")),
            label: Some("LiteLLM telemetry".to_owned()),
            harness: HarnessKind::Litellm,
            workspace: Some(workspace.to_path_buf()),
            session_override: None,
            model: Some(
                ModelName::new("requested-litellm-model").unwrap_or_else(|err| panic!("{err}")),
            ),
            raw_event_retention: RawEventRetention::Disabled,
            execution_permission: meter_core::ExecutionPermissionMode::Standard,
            options: HarnessOptions::empty(),
            prompt: "Do work".to_owned(),
        }
    }

    fn parse_events(telemetry: &str) -> Vec<MeterEvent> {
        telemetry
            .lines()
            .map(|line| serde_json::from_str(line).unwrap_or_else(|err| panic!("{err}: {line}")))
            .collect()
    }
}
