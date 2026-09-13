# Gemini / ChatGPT 接入

AgentBridge 不绑定任何特定模型。Gemini Web、AI Studio 与 ChatGPT 是最需要**远程** Streamable HTTP MCP 端点的一类客户端。

## 本机启动

```bash
agentbridge serve --allow-any-host
```

默认监听 `http://127.0.0.1:8040/mcp`。一旦隧道域名命中 `Host` 头，就必须加 `--allow-any-host`；如果只在本机访问该地址则不需要。

服务启动时会在横幅打印 **Admin PIN**（除非用 `--admin-password` 固定）。ChatGPT 与 Gemini Web 的自定义 MCP 连接会通过 `/oauth/authorize` 完成 OAuth 2.1 重定向，在那里输入 PIN 完成授权。本机可用 `--no-auth` / `--dev` 跳过 401 挑战。

确认服务在线：

```bash
curl http://127.0.0.1:8040/health
```

Web 控制平面在浏览器中位于 `http://127.0.0.1:8040/`。

## 可选隧道

```bash
cloudflared tunnel --url http://127.0.0.1:8040
```

复制 `https://<id>.trycloudflare.com` 地址，MCP 路径为：

```text
https://<id>.trycloudflare.com/mcp
```

## 连接 Brain

在支持 **Remote MCP** 的 Gemini / AI Studio / ChatGPT UI 中：

1. 新增一个名为 `agentbridge` 的服务器。
2. URL 填写上面的 `/mcp` 端点。
3. Transport 选择 Streamable HTTP（有时标注为 “HTTP” 或 `httpUrl`）。
4. 保留 OAuth（默认），在浏览器中用 Admin PIN 完成授权；**或** 启动时设置 `--auth-token`，直接粘贴 `Authorization: Bearer <token>`。

把 [`skill/SKILL.md`](../skill/SKILL.md) 贴入系统指令 / Skill 槽位，使模型以 Brain 身份工作，调用 `task_start` 而不是自己改文件。公网 URL 一律保留 OAuth。

## 第一条提示词

```text
You are the Brain. Use AgentBridge MCP tools only — do not ask me to paste source.

1. Call workspace_info and list_directory on "."
2. Summarize the project.
3. If a code change is needed, produce a compact C2C PLAN and call task_start.
4. Poll task_status, then review git_diff / test_status / execution_summary.
```

## 客户端连不上 MCP 时

- 服务必须在隧道之前、在 Gemini 会话之前启动。
- Host 校验：用 `--allow-any-host` 重启。
- 客户端必须向 `/mcp` POST JSON-RPC，并带 `Accept: application/json, text/event-stream`。
- 快速隧道会过期；重新启动 `cloudflared` 并更新 MCP URL。

Gemini CLI（本地）可以使用同一个 HTTP URL：

```json
{
  "mcpServers": {
    "agentbridge": {
      "httpUrl": "https://<id>.trycloudflare.com/mcp"
    }
  }
}
```
