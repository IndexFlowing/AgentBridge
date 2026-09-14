# Web 管理

Web 是 AgentBridge 的**管理控制面**，不承载核心业务逻辑，也不负责启动任务。所有数据来自 Core / `/api/*`，Web 只做可视化、配置与资源管理。

> CLI / Core 是产品主体；Web 是可选的可视化工具。Desktop / Tray 已废弃。

## 技术栈与打包

| 项 | 说明 |
| --- | --- |
| 框架 | React 18 + react-router-dom 6 |
| 图标 | lucide-react |
| 构建 | esbuild（`web/esbuild.mjs`），入口 `web/src/index.tsx` → `web/dist/bundle.js` |
| 类型 | TypeScript（`tsc --noEmit` 仅类型检查） |
| UI | 无第三方组件库，采用内置 `components/ui` + 内联样式 |
| 打包进二进制 | `rust-embed` 打包 `web/`（排除 `node_modules`、`src`、`*.json`、`*.mjs`），未知路径回退 `index.html`（SPA） |

构建：

```bash
cd web
npm install
npm run build      # 输出 web/dist，由 rust-embed 打包进二进制
npm run typecheck
```

## 页面实现状态

| 页面 | 路由 | 状态 | 能力 |
| --- | --- | --- | --- |
| 仪表盘 | `/` | 已实现（只读） | 网关信息、项目/执行器/任务统计、代理状态、项目列表、当前任务、活动流；手动刷新 |
| 项目 | `/projects` | 已实现 | 挂载 / 列表 / 删除；显示 active、readonly、executor、项目类型、git |
| 执行器 | `/executors` | 已实现 | 登记（当前固定 OpenCode）/ 列表 / 探测 / 测试 / 删除 |
| 技能 | `/skills` | 已实现 | 列出 Agent 根技能与已安装技能；启用 / 禁用 / 移除 / 查看详情 |
| Agent | `/agent` | 已实现（只读） | 查看 Agent 清单、规则、技能及启用状态 |
| Agent Context | `/agent/context` | 已实现（只读诊断） | 选择项目与额外技能，解析某任务会加载的 Rules / Skills |
| 设置 | `/settings` | 已实现 | 查看网关监听配置（只读）；全局代理启用开关 |
| Provider | `/providers` | **占位** | 待实现 |
| 代理 | `/proxy` | **占位** | 待实现（仪表盘与设置已有部分代理信息） |
| 当前任务 | `/tasks` | **占位** | 待实现 |
| 任务历史 | `/tasks/history` | **占位** | 待实现 |
| 安全 / OAuth | `/security` | **占位** | 待实现 |

导航中的「待开发」徽标由 `web/src/navigation.ts` 的 `implemented` 标志驱动。任务相关 REST 只暴露了取消接口，启动 / 查询任务请使用 CLI 或 MCP。

## 管理 API

所有 `/api/*` 接口**仅允许回环地址**访问（Host 必须为 `127.0.0.1` / `localhost` / `[::1]`），与 Brain 使用的 MCP Bearer 分离。

### 系统与代理

| 方法 | 路径 | 作用 |
| --- | --- | --- |
| `GET` | `/api/system/dashboard` | 仪表盘快照 |
| `GET` / `PUT` | `/api/system/connection` | 读取 / 保存连接与认证配置 |
| `GET` | `/api/system/settings` | 系统设置（当前为固定返回，见 [路线图](路线图.md)） |
| `GET` / `PUT` | `/api/proxy` | 读取 / 保存默认代理 |
| `POST` | `/api/proxy/test` | 代理连通性测试 |

### 项目

| 方法 | 路径 | 作用 |
| --- | --- | --- |
| `GET` | `/api/projects` | 项目列表 |
| `POST` | `/api/projects` | 挂载 / 更新项目 |
| `DELETE` | `/api/projects/{id}` | 移除项目 |
| `POST` | `/api/projects/{name}/tasks/cancel` | 取消某项目任务（可选 `task_id`） |

### 执行器

| 方法 | 路径 | 作用 |
| --- | --- | --- |
| `GET` / `POST` | `/api/executors` | 列表 / 保存执行器 |
| `GET` | `/api/executors/available` | 当前可实例化的执行器 |
| `DELETE` | `/api/executors/{id}` | 删除执行器 |
| `POST` | `/api/executors/{id}/test` | 探测 / 测试执行器 |

### 技能与 Agent

| 方法 | 路径 | 作用 |
| --- | --- | --- |
| `GET` / `POST` | `/api/skills` | 列表 / 安装技能 |
| `GET` / `DELETE` | `/api/skills/{name}` | 详情 / 移除 |
| `PUT` | `/api/skills/{name}/enable` / `/disable` | 启用 / 禁用 |
| `GET` | `/api/agent` | Agent 根视图 |
| `GET` | `/api/agent/context` | 解析 AgentContext（诊断） |

### Provider（API 已实现，Web 页面待接入）

| 方法 | 路径 | 作用 |
| --- | --- | --- |
| `GET` / `POST` | `/api/providers` | 列表 / 保存 Provider |
| `GET` | `/api/providers/resolve` | 解析 Provider / Model |
| `DELETE` | `/api/providers/{id}` | 删除 Provider |
| `POST` / `DELETE` | `/api/providers/{id}/credential` | 保存 / 删除凭据（加密存储） |
| `GET` / `POST` | `/api/providers/{id}/models` | 列出 / 保存模型 |
| `DELETE` | `/api/providers/{id}/models/{model_id}` | 删除模型 |

## 相关文档

- [架构](架构.md)
- [Agent体系](Agent体系.md)
- [配置](配置.md)
- [路线图](路线图.md)
