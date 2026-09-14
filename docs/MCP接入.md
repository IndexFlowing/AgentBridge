# MCP 接入

AgentBridge 不绑定任何特定模型。任何支持 **Streamable HTTP MCP** 的客户端都可以作为 Brain 接入：本机 MCP 客户端、ChatGPT、Gemini / AI Studio、Claude 等。

## 启动

```bash
agentbridge serve --allow-any-host
```

默认监听 `http://127.0.0.1:8040/mcp`（发布构建默认端口可能为 8030，见 [配置](配置.md)）。一旦隧道域名命中 `Host` 头，就必须加 `--allow-any-host`；仅本机访问则不需要。

服务启动横幅会打印 **Admin PIN**（除非用 `--admin-password` 固定）。本机可用 `--dev` / `--no-auth` 跳过 401 挑战。

确认服务在线：

```bash
curl http://127.0.0.1:8040/health
```

## MCP 工具

Brain 通过 MCP 与工作区交互。检查类工具严格只读，执行类工具只负责委托。

### 只读（检查与审查）

| 工具 | 说明 |
| --- | --- |
| `list_projects` | 列出已挂载项目及当前激活项目 |
| `switch_project` | 切换本会话的激活项目（仅内存） |
| `workspace_info` | 工作区路径、项目类型、Git 状态 |
| `list_directory` | 防穿越的结构化目录列表 |
| `read_file` | 读取 UTF-8 文本（大小受限，二进制与敏感文件拒绝） |
| `search_workspace` | 关键字搜索，自动忽略 `node_modules`、`target`、`.git` 等 |
| `git_status` | 分支、变更、暂存、未跟踪文件 |
| `git_diff` | 工作区/暂存 diff（`max_diff_bytes` 上限） |
| `test_status` | 最近一次记录在案的测试结果（不会真正运行测试） |
| `execution_summary` | 最近一次迭代的结构化摘要 |
| `list_skills` | 当前项目启用的技能 |
| `agent_config` | Agent 根视图（清单 / 规则 / 技能） |
| `read_skill` | 读取某个 `SKILL.md` 详情 |

### 执行控制

| 工具 | 说明 |
| --- | --- |
| `task_start` | 用校验后的 C2C PLAN 启动本地 Executor |
| `task_status` | 轮询任务生命周期 |
| `task_cancel` | 安全终止正在运行的 Executor 进程树 |

不存在任何写文件、打补丁或通用 shell 工具。检查类与执行类工具都接受可选的 `project` 参数；缺省时使用会话的激活项目。

## 连接方式

### 本机 MCP 客户端（Claude Desktop 等）

```json
{
  "mcpServers": {
    "agentbridge": {
      "command": "npx",
      "args": ["-y", "mcp-remote", "http://127.0.0.1:8040/mcp"]
    }
  }
}
```

本机调试可用 `agentbridge serve --dev` 关闭 401 挑战。

### 远端 Web AI（ChatGPT / Gemini / Claude）

1. 启动：`agentbridge serve --allow-any-host`。
2. 暴露端口（示例）：

   ```bash
   cloudflared tunnel --url http://127.0.0.1:8040
   ```

3. 将 `https://<your-tunnel-id>.trycloudflare.com/mcp` 配置为远端 MCP 地址。
4. 在支持 Remote MCP 的 UI 中新增服务器，Transport 选择 Streamable HTTP。
5. 客户端收到 `401` 后进入 OAuth 流程，在浏览器 `/oauth/authorize` 页面输入 **Admin PIN**。
6. 也可启动时设置 `--auth-token`，直接使用 `Authorization: Bearer <token>`。

最后把 [`skill/SKILL.md`](../skill/SKILL.md) 贴入模型的系统提示 / Skill 槽位，使它以 Brain 身份工作（只检查、只规划、通过 `task_start` 委托执行）。

Gemini CLI（本地）等可使用同一 HTTP URL：

```json
{
  "mcpServers": {
    "agentbridge": {
      "httpUrl": "https://<id>.trycloudflare.com/mcp"
    }
  }
}
```

## OAuth 2.1 端点

| 端点 | 作用 |
| --- | --- |
| `GET /.well-known/oauth-protected-resource` | RFC 9728 资源元数据（含 `mcp:read` / `mcp:write`） |
| `GET /.well-known/oauth-authorization-server` | RFC 8414 授权服务器发现（PKCE S256） |
| `GET /.well-known/openid-configuration` | OpenID 发现别名 |
| `GET/POST /oauth/authorize`（别名 `/authorize`） | 浏览器授权页（Admin PIN）→ 携带 `code`/`state` 重定向 |
| `POST /oauth/token`（别名 `/token`） | 用 `code` + `code_verifier` 换取 access/refresh token |
| `POST /oauth/register`（别名 `/register`） | RFC 7591 动态客户端注册 |
| `POST /oauth/revoke` | 撤销令牌 |

未认证访问 `/mcp` 返回：

```http
HTTP/1.1 401 Unauthorized
WWW-Authenticate: Bearer realm="mcp", resource_metadata="https://<host>/.well-known/oauth-protected-resource"
```

认证与隧道安全细节见 [安全](安全.md)。

## 客户端连不上 MCP 时

- 服务必须在隧道之前、在 AI 会话之前启动。
- Host 校验：用 `--allow-any-host` 重启。
- 客户端必须向 `/mcp` POST JSON-RPC，并带 `Accept: application/json, text/event-stream`。
- 快速隧道会过期；重新启动 `cloudflared` 并更新 MCP URL。

## 第一条提示词

```text
You are the Brain. Use AgentBridge MCP tools only — do not ask me to paste source.

1. Call workspace_info and list_directory on "."
2. Summarize the project.
3. If a code change is needed, produce a compact C2C PLAN and call task_start.
4. Poll task_status, then review git_diff / test_status / execution_summary.
```

## 相关文档

- [C2C协议](C2C协议.md)
- [Agent体系](Agent体系.md)
- [安全](安全.md)
- [配置](配置.md)
