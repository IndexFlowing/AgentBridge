use crate::config::ProxyConfig;
use crate::executor::ExecutorError;

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

pub async fn test_proxy(proxy: &ProxyConfig) -> Result<String, ExecutorError> {
    proxy
        .validate()
        .map_err(|e| ExecutorError::Other(e.to_string()))?;
    let url = proxy
        .url()
        .map_err(|e| ExecutorError::Other(e.to_string()))?;
    let client = reqwest::Client::builder()
        .proxy(
            reqwest::Proxy::all(&url)
                .map_err(|_| ExecutorError::Other("invalid proxy settings".into()))?,
        )
        .build()
        .map_err(|_| ExecutorError::Other("could not create proxy test client".into()))?;
    let response = client
        .get("https://example.com")
        .send()
        .await
        .map_err(|_| ExecutorError::Other("proxy connection failed".into()))?;
    if response.status().is_success() {
        Ok("代理连接成功，HTTPS 访问可用".into())
    } else {
        Err(ExecutorError::Other(format!(
            "代理已连接，但 HTTPS 返回状态 {}",
            response.status().as_u16()
        )))
    }
}
