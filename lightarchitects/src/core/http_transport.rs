//! HTTP transport — connects the SDK to the Light Architects cloud API.
//!
//! [`HttpTransport`] implements [`Transport`] by posting JSON-RPC requests to
//! `POST /v1/{sibling}/{action}` on a remote gateway (default:
//! `https://api.lightarchitects.ai`). Business logic stays on the server;
//! the SDK is a thin typed client.
//!
//! # Feature gate
//!
//! Requires the `http-client` feature (enabled by default).

use std::time::Duration;

use reqwest::Client;

use crate::core::error::{SdkError, TransportError};
use crate::core::jsonrpc::{JsonRpcRequest, JsonRpcResponse};
use crate::core::sibling::SiblingId;
use crate::core::transport::Transport;

/// Default gateway base URL.
pub const DEFAULT_BASE_URL: &str = "https://api.lightarchitects.ai";

/// Default HTTP request timeout.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// HTTP transport that forwards JSON-RPC requests to the Light Architects
/// cloud gateway instead of spawning a local sibling binary.
///
/// Cheap to clone — the inner [`reqwest::Client`] is `Arc`-backed.
///
/// # Authentication
///
/// Pass your API key via [`HttpTransportBuilder::api_key`]. The key is sent as
/// `Authorization: Bearer <key>` on every request.
///
/// # Example
///
/// ```no_run
/// use lightarchitects::core::HttpTransport;
/// use lightarchitects::core::sibling::SiblingId;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let transport = HttpTransport::builder(SiblingId::Soul)
///     .api_key("la_your_key_here")
///     .build()?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct HttpTransport {
    client: Client,
    base_url: String,
    api_key: String,
    sibling: SiblingId,
}

impl HttpTransport {
    /// Start building an [`HttpTransport`] for the given sibling.
    #[must_use]
    pub fn builder(sibling: SiblingId) -> HttpTransportBuilder {
        HttpTransportBuilder::new(sibling)
    }

    /// Construct the endpoint URL for the given JSON-RPC method.
    ///
    /// Gateway route: `POST /v1/{sibling}/{action}`
    /// where `sibling` is lowercase and `action` is the MCP tool name.
    fn endpoint_url(&self, method: &str) -> String {
        let sibling = self.sibling.name().to_lowercase();
        format!("{}/v1/{sibling}/{method}", self.base_url)
    }
}

impl Transport for HttpTransport {
    // `impl Future` return matches the Transport trait signature. Keeping them
    // in sync is deliberate; an `async fn` here would diverge from the trait.
    #[allow(clippy::manual_async_fn)]
    fn send(
        &self,
        request: JsonRpcRequest,
    ) -> impl std::future::Future<Output = Result<JsonRpcResponse, SdkError>> + Send + '_ {
        async move {
            let url = self.endpoint_url(&request.method);
            let resp = self
                .client
                .post(&url)
                .bearer_auth(&self.api_key)
                .json(&request)
                .send()
                .await
                .map_err(|e| SdkError::Transport(TransportError::Http(e.to_string())))?;

            if !resp.status().is_success() {
                let status = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                return Err(SdkError::Transport(TransportError::Http(format!(
                    "HTTP {status}: {body}"
                ))));
            }

            resp.json::<JsonRpcResponse>()
                .await
                .map_err(|e| SdkError::Transport(TransportError::Http(e.to_string())))
        }
    }
}

/// Builder for [`HttpTransport`].
#[derive(Debug)]
pub struct HttpTransportBuilder {
    sibling: SiblingId,
    api_key: String,
    base_url: String,
    timeout: Duration,
}

impl HttpTransportBuilder {
    fn new(sibling: SiblingId) -> Self {
        Self {
            sibling,
            api_key: String::new(),
            base_url: DEFAULT_BASE_URL.to_owned(),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// Set the API key (required).
    #[must_use]
    pub fn api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = key.into();
        self
    }

    /// Override the gateway base URL (default: `https://api.lightarchitects.ai`).
    #[must_use]
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// Override the HTTP request timeout (default: 30 s).
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Build the [`HttpTransport`].
    ///
    /// # Errors
    ///
    /// Returns [`SdkError::Config`] if the API key is empty or the HTTP client
    /// cannot be constructed.
    pub fn build(self) -> Result<HttpTransport, SdkError> {
        if self.api_key.is_empty() {
            return Err(SdkError::Config(
                "api_key is required for HttpTransport".into(),
            ));
        }
        let client = Client::builder()
            .timeout(self.timeout)
            .build()
            .map_err(|e| SdkError::Config(format!("failed to build HTTP client: {e}")))?;
        Ok(HttpTransport {
            client,
            base_url: self.base_url,
            api_key: self.api_key,
            sibling: self.sibling,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn builder_rejects_empty_api_key() {
        let result = HttpTransport::builder(SiblingId::Soul).build();
        assert!(matches!(result, Err(SdkError::Config(_))));
    }

    #[test]
    fn endpoint_url_lowercase_sibling() {
        let transport = HttpTransport::builder(SiblingId::Soul)
            .api_key("la_test")
            .base_url("https://api.example.io")
            .build()
            .unwrap();
        assert_eq!(
            transport.endpoint_url("search"),
            "https://api.example.io/v1/soul/search"
        );
    }

    #[test]
    fn endpoint_url_corso() {
        let transport = HttpTransport::builder(SiblingId::Corso)
            .api_key("la_test")
            .build()
            .unwrap();
        assert!(transport.endpoint_url("sniff").contains("/v1/corso/sniff"));
    }
}
