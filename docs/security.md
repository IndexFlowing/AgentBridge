# Security

AgentBridge exposes a **read-only** view of one workspace to a remote AI. Treat that as data leaving your machine.

It is not a hardened multi-tenant product. Do not claim it is “secure” without qualification.

## Defaults

- Bind `127.0.0.1` only.
- MCP tools never write files or run commands.
- Paths are resolved under the configured workspace. `../`, absolute paths, and `~` are rejected when they escape.
- Existing paths are canonicalized so symlinks cannot walk out of the workspace.
- Sensitive names are denied: `.env`, `.env.*`, `*.pem`, `*.key`, `id_rsa`, `id_ed25519`, `credentials*`, `secrets*`.
- Home trees `~/.ssh`, `~/.aws`, `~/.config`, `~/.docker`, and `~/.npmrc` are denied even if a path trick lands there.
- File reads are UTF-8 text only, with a size cap (1 MiB by default).
- Search skips `.git`, `node_modules`, `target`, `dist`, `build`, `.cache`.

## Host header

The MCP HTTP stack rejects non-loopback `Host` headers by default (DNS rebinding). Cloudflare Tunnel presents a `*.trycloudflare.com` Host, so you must start the server with:

```bash
agentbridge serve --allow-any-host
```

That flag disables Host allowlisting. Combine it with an auth token if the URL might leak:

```bash
agentbridge serve --allow-any-host --auth-token "$TOKEN"
```

Set the matching `Authorization: Bearer` header in the MCP client.

## Tunneling

`cloudflared tunnel --url http://127.0.0.1:8787` publishes your MCP endpoint to the internet for the lifetime of the process. Anyone with the URL can read the workspace (and call tools) unless you set `auth_token`.

Do not tunnel a home directory, a secrets repo, or a workspace that contains production credentials.

## What this does not do (V0.1)

- No OAuth
- No per-file ACLs beyond the deny list
- No encryption beyond whatever the tunnel provides
- No audit log of tool calls
- No sandbox around the Executor — the Executor is local and fully privileged

## Recommendations

1. Point `workspace` at a single project, never `$HOME`.
2. Keep `host = "127.0.0.1"`.
3. Review `list_directory` / `search_workspace` before inviting a remote Brain.
4. Use `--auth-token` for any public URL.
5. Stop the tunnel when you are done reviewing.
