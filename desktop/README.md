# AgentBridge Desktop

The `desktop/` directory is the first-stage Tauri 2 shell for AgentBridge. It is intentionally separate from the existing Rust CLI and egui tray console; `agentbridge tray` remains the compatibility entry point.

## Development

```bash
npm install
npm run dev
```

To run the Tauri shell after installing the Rust toolchain:

```bash
npm run tauri dev
```

Build the frontend with `npm run build`. The current UI uses view-model data so it can be developed without a running gateway. Native calls are isolated in `src/ipc.ts`; the minimal `desktop_snapshot` command in `src-tauri/src/lib.rs` is the seam for wiring the existing Rust core in a later phase.

## Migration boundary

The existing `src/*.rs` modules, CLI commands, MCP server, OAuth, executor, and egui tray are not copied or replaced here. Future IPC commands should call those existing modules or a shared library boundary rather than reimplementing business logic in React.
