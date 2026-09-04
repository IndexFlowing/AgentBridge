# Gemini / AI Studio

AgentBridge is not Gemini-specific. Gemini Web and AI Studio are the first clients that need a **remote** Streamable HTTP MCP endpoint.

## Local server

```bash
agentbridge init ~/projects/my-project
agentbridge serve --allow-any-host
```

`--allow-any-host` is required once a tunnel hostname hits the `Host` header. Leave it off if you only talk to `http://127.0.0.1:8030/mcp` from the same machine.

The server prints an **Admin PIN** at startup. ChatGPT and Gemini Web custom MCP connections run the OAuth 2.1 redirect against `/oauth/authorize`; enter that PIN to approve. `--no-auth` / `--dev` skips the 401 challenge on localhost.

Confirm:

```bash
curl http://127.0.0.1:8787/health
```

## Optional tunnel

```bash
cloudflared tunnel --url http://127.0.0.1:8787
```

Copy the `https://<id>.trycloudflare.com` URL. The MCP path is:

```
https://<id>.trycloudflare.com/mcp
```

## Connect the Brain

In a Gemini / AI Studio UI that supports **Remote MCP**:

1. Add a server named `agentbridge`.
2. URL: the `/mcp` endpoint above.
3. Transport: Streamable HTTP (sometimes labeled “HTTP” or `httpUrl`).
4. Leave OAuth enabled (default). Complete the in-browser approval with the Admin PIN, **or** paste a static `Authorization: Bearer <token>` if you started with `--auth-token`.

Paste `skill/SKILL.md` into the system instructions / skill slot so the model behaves as the Brain.

The same URL works for ChatGPT Web (Remote MCP / Streamable HTTP). Paste `skill/SKILL.md` so the model calls `task_start` instead of editing files itself. Leave OAuth on whenever the URL is public.

## First prompt

```
You are the Brain. Use AgentBridge MCP tools only — do not ask me to paste source.

1. Call workspace_info and list_directory on "."
2. Summarize the project.
3. If a code change is needed, produce a compact C2C PLAN and call task_start.
4. Poll task_status, then review git_diff / test_status / execution_summary.
```

## If the client cannot reach MCP

- The server must be running **before** the tunnel and before the Gemini session.
- Host validation: restart with `--allow-any-host`.
- The client must POST JSON-RPC to `/mcp` with `Accept: application/json, text/event-stream`.
- Quick tunnels expire; start a new `cloudflared` and update the MCP URL.

Gemini CLI (local) can use the same HTTP URL:

```json
{
  "mcpServers": {
    "agentbridge": {
      "httpUrl": "https://<id>.trycloudflare.com/mcp"
    }
  }
}
```
