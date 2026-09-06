pub fn consent_page(request_id: &str, client: &str, scope: Option<&str>) -> String {
    consent_page_inner(request_id, client, scope, None)
}

pub fn consent_page_error(request_id: &str, client: &str, error: &str) -> String {
    consent_page_inner(request_id, client, None, Some(error))
}

fn consent_page_inner(
    request_id: &str,
    client: &str,
    scope: Option<&str>,
    error: Option<&str>,
) -> String {
    let scopes = scope.unwrap_or("mcp:read mcp:write");
    let error_html = error
        .map(|e| format!("<div class=\"err-box\">✕ {}</div>", html_escape(e)))
        .unwrap_or_default();

    format!(
        r##"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8"/>
  <meta name="viewport" content="width=device-width, initial-scale=1"/>
  <title>授权接入 AgentBridge</title>
  <style>
    :root {{ color-scheme: dark; }}
    * {{ box-sizing: border-box; }}
    body {{
      margin: 0; min-height: 100vh; display: grid; place-items: center;
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", sans-serif;
      background: #0b1015; color: #e7ecf1; padding: 1rem;
    }}
    .card {{
      width: 100%; max-width: 26rem;
      background: #141c24; border: 1px solid #242e3a; border-radius: 16px;
      padding: 2rem 1.75rem 1.75rem; box-shadow: 0 24px 60px rgba(0,0,0,.45);
      overflow: hidden; word-break: break-word; overflow-wrap: anywhere;
    }}
    .brand {{
      display: flex; align-items: center; gap: 8px;
      letter-spacing: .08em; text-transform: uppercase; font-size: .75rem; color: #3dd6c6; font-weight: 700;
      margin-bottom: 1rem;
    }}
    h1 {{ font-size: 1.35rem; margin: 0 0 .5rem; font-weight: 700; color: #ffffff; }}
    p.desc {{ color: #9aa6b2; line-height: 1.5; font-size: .92rem; margin: 0 0 1.25rem; }}
    p.desc strong {{ color: #3dd6c6; font-weight: 600; }}
    
    .scope-card {{
      background: #0e141a; border: 1px solid #1f2833; border-radius: 10px;
      padding: .75rem 1rem; margin-bottom: 1.25rem; font-size: .82rem; color: #7f8b98;
    }}
    .scope-title {{ font-weight: 600; color: #c5d1de; margin-bottom: .25rem; font-size: .85rem; }}
    .scopes {{ font-family: ui-monospace, SFMono-Regular, Consolas, monospace; color: #3dd6c6; font-size: .78rem; }}

    label {{ display: block; font-size: .85rem; margin: 0 0 .45rem; color: #c5d1de; font-weight: 500; }}
    input[type=password] {{
      width: 100%; padding: .75rem 1rem; border-radius: 10px;
      border: 1px solid #2a3644; background: #0b1015; color: #ffffff; font-size: 1rem;
      outline: none; transition: border-color .2s;
    }}
    input[type=password]:focus {{ border-color: #3dd6c6; }}

    .actions {{ display: flex; gap: .75rem; margin-top: 1.5rem; }}
    button {{
      flex: 1; padding: .75rem 1rem; border-radius: 10px; border: 0; cursor: pointer;
      font-weight: 600; font-size: .95rem; transition: opacity .2s;
    }}
    button:hover {{ opacity: .9; }}
    .ok {{ background: #3dd6c6; color: #06231f; }}
    .no {{ background: #222b36; color: #c5d1de; }}

    .err-box {{
      background: #2a1515; border: 1px solid #5c2b2b; color: #ff8d8d;
      padding: .65rem .85rem; border-radius: 8px; font-size: .85rem; margin-bottom: 1.2rem;
      display: flex; align-items: center; gap: 6px;
    }}
  </style>
</head>
<body>
  <form class="card" method="post" action="/oauth/authorize">
    <div class="brand">
      <span>✦</span> AgentBridge MCP
    </div>
    <h1>授权接入请求</h1>
    <p class="desc"><strong>{client}</strong> 正在请求访问本机的本地代码工作区与执行器。</p>

    <div class="scope-card">
      <div class="scope-title">请求的访问权限：</div>
      <div class="scopes">{scopes}</div>
    </div>

    {error}

    <input type="hidden" name="request_id" value="{rid}"/>
    <label for="password">输入管理密码 (Admin Password)</label>
    <input id="password" name="password" type="password" autocomplete="current-password" placeholder="请输入你的密码" required autofocus/>

    <div class="actions">
      <button class="no" type="submit" name="action" value="deny" formnovalidate>拒绝</button>
      <button class="ok" type="submit" name="action" value="approve">确认授权</button>
    </div>
  </form>
</body>
</html>
"##,
        client = html_escape(client),
        scopes = html_escape(scopes),
        error = error_html,
        rid = html_escape(request_id),
    )
}

pub fn idle_page() -> String {
    r#"<!doctype html><meta charset=utf-8><title>AgentBridge OAuth</title>
<body style="font-family:-apple-system,BlinkMacSystemFont,sans-serif;max-width:36rem;margin:4rem auto;color:#e7ecf1;background:#0b1015;padding:1rem;">
<h1>AgentBridge MCP 网关就绪</h1>
<p>ChatGPT 或 Gemini 在执行 MCP OAuth 2.1 鉴权时会自动调起此页面。请在 AI 客户端中发起连接以开始。</p>
</body>"#
        .into()
}

pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}