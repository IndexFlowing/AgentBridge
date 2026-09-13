# 安全

AgentBridge 把本地工作区暴露给远端 AI，并允许该 AI **启动本地 OpenCode 进程**。两者都是真实的信任决策，请谨慎对待。

它不是经过强化的多租户产品，请不要在无保留的情况下宣称它“安全”。

## 默认安全策略

- 默认仅绑定 `127.0.0.1`。
- MCP 检查类工具绝不写文件、绝不运行命令。
- 执行控制类工具（`task_start` / `task_status` / `task_cancel`）不接受 shell 命令，也不接受可执行文件名。
- OpenCode 的命令来自本机注册表（Web 控制平面写入 SQLite），绝不来自 MCP 请求。
- 执行器类型受白名单约束；当前**只有 OpenCode 会真正启动**，Codex / Claude 等仅可登记与探测。
- Executor 的工作目录是所选项目根目录。
- 路径在**所选项目**根目录下解析。`../`、绝对路径、`~` 一旦逃出该项目即被拒绝；`project` 参数不能用来读取同级工作区。
- 已存在的路径会被规范化，符号链接无法走出工作区。
- 拒绝敏感文件名：`.env`、`.env.*`、`*.pem`、`*.key`、`id_rsa`、`id_ed25519`、`credentials*`、`secrets*`。
- 即使通过路径技巧落到家目录，也拒绝 `~/.ssh`、`~/.aws`、`~/.config`、`~/.docker`、`~/.npmrc`。
- 文件读取仅限 UTF-8 文本，默认上限 1 MiB。
- 搜索跳过 `.git`、`node_modules`、`target`、`dist`、`build`、`.cache`、`.agentbridge`。
- Executor 输出会被截断；`<think>` 等推理内容会被剥离且不返回给 Brain。
- 日志不记录用户输入或系统的 Admin PIN，也不回显 access / refresh token 明文。

## 管理接口隔离

`/api/*` 是 Web 控制平面使用的管理接口，**仅允许回环地址**访问。远端 `Host` 访问管理接口会被拒绝，因此即使 MCP URL 泄漏，也无法通过该 Token 修改本机项目与设置。

## Host 头校验

MCP HTTP 栈默认拒绝非回环 `Host`（防 DNS rebinding）。Cloudflare Tunnel 会带来 `*.trycloudflare.com` 的 Host，因此需要通过隧道访问时必须：

```bash
agentbridge serve --allow-any-host
```

该参数会关闭 Host 白名单。请同时保留 OAuth（默认开启）或设置静态 Token：

```bash
agentbridge serve --allow-any-host
# 或同时固定静态 Token：
agentbridge serve --allow-any-host --auth-token "$TOKEN"
```

ChatGPT 与 Gemini Web 会走 `/oauth/authorize` 与 `/oauth/token` 完成 OAuth 2.1；静态 `Authorization: Bearer` 同样可用。

**任何隧道场景都应保留 OAuth 或设置 `--auth-token`。** 公网 MCP URL 不仅能读取文件，还能启动本机 OpenCode。仅在回环环境使用 `--no-auth` / `--dev`。

## 隧道

`cloudflared tunnel --url http://127.0.0.1:8040` 会在进程存活期间把 MCP 端点发布到公网。除非设置 OAuth 或 `auth_token`，否则任何拿到 URL 的人都能读取工作区**并启动 Executor**。

不要把家目录、密钥仓库或含生产凭据的工作区通过隧道暴露。

## 本版本不提供的保证

- 除拒绝名单外，没有逐文件 ACL。
- 除隧道本身外，没有额外的传输加密。
- 没有工具调用的审计日志。
- 没有围绕 OpenCode 的操作系统级沙箱：Executor 是本地进程，在所选项目内（甚至项目外，若它自行越界）拥有完全权限。AgentBridge 只设置 cwd 并禁止 MCP 选择二进制，并不会 jail 子进程。

## 建议

1. 每个项目只指向一个仓库，绝不要指向 `$HOME`。
2. 保持 `host = "127.0.0.1"`。
3. 邀请远端 Brain 前，先用 `list_directory` / `search_workspace` 检查暴露面。
4. 公网 URL 一律保留 OAuth 或设置 `--auth-token`。
5. 审查结束后关闭隧道。
6. 运行异常时用 `task_cancel` 终止 OpenCode 进程树。
