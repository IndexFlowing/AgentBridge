use crate::config::ProxyConfig;
use crate::executor::ExecutorError;

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