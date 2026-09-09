use axum::extract::Request;
use axum::http::{header, Method, StatusCode};
use axum::middleware::Next;
use axum::response::Response;

pub async fn mcp_diagnostics_layer(req: Request, next: Next) -> Response {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let path = uri.path().to_string();
    let is_mcp = path.starts_with("/mcp");

    let session_id = req.headers().get("mcp-session-id").and_then(|v| v.to_str().ok()).map(ToString::to_string);
    let host = req.headers().get("host").and_then(|v| v.to_str().ok()).unwrap_or("-").to_string();
    let auth = req.headers().get("authorization").and_then(|v| v.to_str().ok()).map(|s| if s.len() > 14 { format!("{}...", &s[..14]) } else { s.to_string() });

    // 只有非失效重复的请求才打印，保持控制台清爽
    if is_mcp && method != Method::GET {
        eprintln!("[MCP-Gateway] ──> {} {} | Host: {} | Session: {:?} | Auth: {:?}", method, path, host, session_id, auth);
    }

    let start = std::time::Instant::now();
    let response = next.run(req).await;
    let elapsed = start.elapsed();
    let status = response.status();

    if is_mcp && method != Method::GET {
        eprintln!("[MCP-Gateway] <-- {} {} => Status: {} ({:?})", method, path, status, elapsed);
    }

    if is_mcp && status == StatusCode::FORBIDDEN {
        eprintln!("[MCP-Gateway] ❌ Host 拦截 (403): Host='{}'. 如需使用 Tunnel，请设置: allow_any_host = true", host);
    }

    // 核心会话自愈与死循环终结
    if is_mcp && status == StatusCode::NOT_FOUND && session_id.is_some() {
        if method == Method::GET {
            // 依据 W3C SSE 规范：返回 204 No Content 命令客户端彻底终止重试循环，消除刷屏
            return Response::builder()
                .status(StatusCode::NO_CONTENT)
                .body(axum::body::Body::empty())
                .unwrap_or(response);
        } else {
            // 对于 POST 请求（JSON-RPC）：返回合法错误格式，阻止 ChatGPT 抛出 502
            eprintln!("[MCP-Gateway] ⚠️ 拦截到失效 Session 的 POST 请求: {:?} => 安全降级", session_id);
            let safe_json = serde_json::json!({
                "jsonrpc": "2.0",
                "error": { "code": -32000, "message": "AgentBridge service restarted. The previous session has ended. Please refresh." }
            });
            return Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(safe_json.to_string()))
                .unwrap_or(response);
        }
    }

    response
}