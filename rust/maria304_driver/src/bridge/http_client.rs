//! Blocking HTTP [`Bridge`] implementation.
//!
//! Posts canonical envelopes to the Python PRRO Gateway via
//! `POST /v1/ingress/maria304`.  Uses `reqwest::blocking` because the
//! [`Bridge`] trait is synchronous — the listener's per-connection
//! async task blocks while this call is in flight.  Single-connection-
//! per-FN semantics mean there is no lost throughput.
//!
//! Auth is a simple bearer token (`Authorization: Bearer <token>`)
//! pulled from config.  TLS is opt-in via `https://` URLs and uses
//! rustls (no openssl dependency).

use std::time::Duration;

use reqwest::blocking::Client;

use super::{Bridge, BridgeError, CanonicalCommand, CanonicalResponse};

/// Real HTTP bridge client.  Lives for the whole process lifetime —
/// the underlying connection pool is shared across all
/// per-connection tasks on the same `FnListener`.
#[derive(Debug)]
pub struct HttpBridge {
    client: Client,
    gateway_url: String,
    shared_token: String,
}

impl HttpBridge {
    /// Build an HTTP bridge.
    ///
    /// * `gateway_url` — full URL including `/v1/ingress/maria304`.
    /// * `shared_token` — bearer token injected into the
    ///   `Authorization` header.  Empty string skips auth (dev only).
    /// * `connect_timeout` / `request_timeout` — conservative values
    ///   from config.
    ///
    /// # Errors
    /// Returns the wrapped `reqwest::Error` if the HTTP client cannot
    /// be built (invalid TLS config, invalid URL, etc.).
    pub fn new(
        gateway_url: impl Into<String>,
        shared_token: impl Into<String>,
        connect_timeout: Duration,
        request_timeout: Duration,
    ) -> Result<Self, reqwest::Error> {
        let client = Client::builder()
            .connect_timeout(connect_timeout)
            .timeout(request_timeout)
            .user_agent("maria304_driver/0.1.0")
            .build()?;
        Ok(Self {
            client,
            gateway_url: gateway_url.into(),
            shared_token: shared_token.into(),
        })
    }
}

impl Bridge for HttpBridge {
    fn submit(&self, command: &CanonicalCommand) -> Result<CanonicalResponse, BridgeError> {
        let mut req = self.client.post(&self.gateway_url).json(command);
        if !self.shared_token.is_empty() {
            req = req.bearer_auth(&self.shared_token);
        }
        let response = match req.send() {
            Ok(r) => r,
            Err(e) => {
                return Err(BridgeError::Transport(format!("reqwest: {e}")));
            }
        };

        let status = response.status();
        if status.is_success() {
            response
                .json::<CanonicalResponse>()
                .map_err(|e| BridgeError::Transport(format!("json decode: {e}")))
        } else {
            // Try to decode `{"ok": false, "error_code": "...", ...}`
            // — fall back to a generic rejected if the body is
            // unparseable.
            #[derive(serde::Deserialize)]
            struct ErrorBody {
                #[serde(default)]
                error_code: String,
                #[serde(default)]
                error_message: String,
            }
            match response.json::<ErrorBody>() {
                Ok(e) => Err(BridgeError::Rejected {
                    code: if e.error_code.is_empty() {
                        format!("HTTP_{}", status.as_u16())
                    } else {
                        e.error_code
                    },
                    message: e.error_message,
                }),
                Err(_) => Err(BridgeError::Rejected {
                    code: format!("HTTP_{}", status.as_u16()),
                    message: String::new(),
                }),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_rejects_empty_url_lazily_not_at_construction() {
        // Construction only fails for invalid TLS or similar — URL
        // validation happens at request time.  Smoke test that the
        // builder returns Ok for a well-formed but not-yet-probed
        // local URL.
        let bridge = HttpBridge::new(
            "http://127.0.0.1:1/v1/ingress/maria304",
            "",
            Duration::from_millis(100),
            Duration::from_millis(100),
        );
        assert!(bridge.is_ok());
    }

    #[test]
    fn submit_to_unreachable_host_yields_transport_error() {
        // Port 1 is almost never listening — connection refused maps
        // to BridgeError::Transport so dispatcher replies SOFTBLOCK.
        let bridge = HttpBridge::new(
            "http://127.0.0.1:1/v1/ingress/maria304",
            "",
            Duration::from_millis(100),
            Duration::from_millis(500),
        )
        .unwrap();

        let cmd = CanonicalCommand {
            schema_version: "1.0".to_string(),
            fiscal_number: "F".to_string(),
            command_type: super::super::dto::CommandType::Sell,
            idempotency_key: "k".to_string(),
            cashier_id: None,
            department: None,
            return_check_number: None,
            payload: super::super::dto::ReceiptPayload::default(),
        };
        let err = bridge.submit(&cmd).unwrap_err();
        assert!(
            matches!(err, BridgeError::Transport(_)),
            "expected Transport error, got {err:?}",
        );
    }

    #[test]
    fn http_bridge_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<HttpBridge>();
    }
}
