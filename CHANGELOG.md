# Changelog

本文件记录 AgentBridge 面向用户的版本变更。后续版本按发布日期倒序添加，并使用以下分类：

- **Added**：新增能力
- **Changed**：现有行为或界面调整
- **Fixed**：问题修复

## [0.5.1] - 2026-09-05

本版本完成桌面端发布前稳定性与一致性校准。

### Added

- 桌面端 UI
- Projects CRUD 与原生文件夹选择
- Executor Proxy 配置

### Changed

- MCP 默认监听端口统一为 8040，同时保留显式端口配置
- Windows 桌面端使用无控制台 GUI 子系统，并保留自定义标题栏的窗口控制与拖拽权限

### Fixed

- 修复桌面端连接探测、项目管理与发布配置相关的稳定性问题

## [0.5.0] - 人工试用基线

这是 AgentBridge 进入人工试用阶段的基线版本。

### Added

- MCP 接入与 OAuth 身份验证
- 项目与工作区管理
- Task 生命周期记录
- OpenCode Executor
- Executor 网络 Proxy 配置
- Tauri 桌面端控制台
- 并发隔离测试

## 后续版本格式

## [0.6.0] - YYYY-MM-DD

### Added

-

### Changed

-

### Fixed

-
