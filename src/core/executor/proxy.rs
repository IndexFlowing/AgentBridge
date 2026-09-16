use std::time::{Duration, Instant};

use crate::config::ProxyConfig;
use crate::executor::ExecutorError;

/// Stable built-in probe target used when a proxy does not define its own
/// `test_url`. It is a plain, credential-free HTTPS endpoint.
pub const DEFAULT_TEST_URL: &str = "https://example.com";

/// Reasonable upper bound for a user-triggered connectivity probe.
pub const DEFAULT_TEST_TIMEOUT: Duration = Duration::from_secs(8);

/// Inject a resolved [`ProxyConfig`] as the conventional proxy environment
/// variables. This is the single point where a network proxy reaches an
/// executor process; executors never build proxy URLs themselves.
pub fn apply_proxy_env(
    cmd: &mut std::process::Command,
    proxy: Option<&ProxyConfig>,
) -> Result<(), ExecutorError> {
    let Some(proxy) = proxy else {
        return Ok(());
    };
    if !proxy.enabled {
        return Ok(());
    }
    let url = proxy
        .url()
        .map_err(|e| ExecutorError::Other(e.to_string()))?;
    cmd.env("HTTP_PROXY", &url)
        .env("HTTPS_PROXY", &url)
        .env("ALL_PROXY", &url)
        .env("NO_PROXY", "localhost,127.0.0.1,::1");
    Ok(())
}

/// Outcome of a real, proxy-routed connectivity probe.
#[derive(Debug, Clone)]
pub struct ProxyTestReport {
    pub success: bool,
    pub latency_ms: u64,
    pub message: String,
    pub target: String,
}

/// Actually route a request through `proxy` to `target` and report the result.
///
/// Unlike a string-format check, this opens a real connection through the
/// configured proxy. Errors never echo credentials: only the status code or a
/// generic reason is surfaced.
pub async fn test_proxy(
    proxy: &ProxyConfig,
    target: &str,
    timeout: Duration,
) -> Result<ProxyTestReport, ExecutorError> {
    proxy
        .validate()
        .map_err(|e| ExecutorError::Other(e.to_string()))?;
    let url = proxy
        .url()
        .map_err(|e| ExecutorError::Other(e.to_string()))?;
    let target = if target.trim().is_empty() {
        DEFAULT_TEST_URL
    } else {
        target.trim()
    };
    if !(target.starts_with("http://") || target.starts_with("https://")) {
        return Err(ExecutorError::Other(
            "proxy test target must be an http(s) URL".into(),
        ));
    }

    let client = reqwest::Client::builder()
        .proxy(
            reqwest::Proxy::all(&url)
                .map_err(|_| ExecutorError::Other("invalid proxy settings".into()))?,
        )
        .timeout(timeout)
        .build()
        .map_err(|_| ExecutorError::Other("could not create proxy test client".into()))?;

    let started = Instant::now();
    let outcome = client.get(target).send().await;
    let latency_ms = started.elapsed().as_millis() as u64;

    let (success, message) = match outcome {
        Ok(response) => {
            let status = response.status();
            if status.is_success() || status.as_u16() == 204 {
                (true, "代理连接成功，HTTPS 访问可用".to_string())
            } else {
                (
                    false,
                    format!("代理已连接，但目标返回状态 {}", status.as_u16()),
                )
            }
        }
        Err(err) => {
            let reason = if err.is_timeout() {
                "代理连接超时".to_string()
            } else if err.is_connect() {
                "无法通过该代理建立连接（请检查地址、端口与协议）".to_string()
            } else {
                "代理连接失败".to_string()
            };
            (false, reason)
        }
    };

    Ok(ProxyTestReport {
        success,
        latency_ms,
        message,
        target: target.to_string(),
    })
}
