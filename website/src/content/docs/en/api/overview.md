---
title: API Overview
description: The FluxDown HTTP API — route groups, authentication, and how it relates to the headless server.
section: api
order: 1
---

FluxDown ships a small HTTP API — used by browser extensions, userscripts, aria2 clients, and automation — built into two places:

- **The desktop app**, on `http://127.0.0.1:17800` (port configurable, address hardcoded to loopback: it is never reachable from the network). It's off by default for the management group and on by default for the other groups; see the desktop client's local API settings.
- **The [headless server](/docs/en/headless-server/setup/)**, on whatever address `FLUXDOWN_BIND` is set to (`0.0.0.0:17800` by default — reachable over the network by design, since remote management is the point). The management API is always enabled there, and it adds a handful of server-specific endpoints (queues, config, file retrieval, WebSocket, filesystem browsing) beyond what the desktop app exposes.

Both share the same underlying route constants, request/response JSON contracts, and auth rules — only which routes are enabled, and what host implements them, differs.

## Route groups

| Group | Endpoints | Enabled by | Authentication |
|---|---|---|---|
| Health check | `GET /ping` | always on | none |
| Script takeover | `POST /download`, `POST /download/batch` | `local_server_takeover_enabled` (default on) | `X-FluxDown-Client` header required, plus an optional token |
| aria2-compatible RPC | `POST /jsonrpc` (`aria2.addUri`, `aria2.getVersion`, `aria2.getGlobalStat`, `system.multicall`, `system.listMethods`) | `local_server_jsonrpc_enabled` (default on) | optional token |
| Management API | `GET /api/v1/info`, `GET/POST /api/v1/tasks`, `GET/DELETE /api/v1/tasks/{id}`, `PUT /api/v1/tasks/{id}/pause\|continue`, `PUT /api/v1/tasks/pause\|continue`, `GET /api/v1/queues` | `local_server_api_enabled` (default off on desktop, always on for the headless server) | **required** token |

`GET /api/v1/openapi.json` (no auth — it's a pure interface description with no data) is available whenever the management group is enabled.

On the headless server specifically, `/api/v1/*` also includes extra routes not present on the desktop app: `GET /api/v1/ws` (WebSocket), `GET/PUT /api/v1/config`, `POST/PUT/DELETE /api/v1/queues[/{id}]`, `PUT /api/v1/tasks/{id}/queue`, `PUT /api/v1/tasks/{id}/boost`, `GET /api/v1/tasks/{id}/file`, `GET /api/v1/fs/list`, `POST /api/v1/proxy/test`, `POST /api/v1/token/regenerate`, and `GET /api/v1/stats`. These follow the same token rules as the rest of the management group, except `/ws` and `/tasks/{id}/file` (browser-initiated requests can't set custom headers, so both take `?token=` as a query parameter instead) and `/openapi.json`/`/docs` (unauthenticated).

## Authentication

There is one configured token (`local_server_token`); how it must be presented depends on the route group:

| Route group | Accepted forms |
|---|---|
| Script takeover | `X-FluxDown-Token` header (only if a token is configured — empty token means the group is unauthenticated). The `X-FluxDown-Client` header is always required regardless of token, as a CORS-based gate against arbitrary web pages. |
| aria2-compatible RPC | `X-FluxDown-Token` header, **or** aria2's own convention of passing `token:xxx` as `params[0]` in the JSON-RPC call. |
| Management API (`/api/v1/*`) | `Authorization: Bearer <token>` **or** `X-FluxDown-Token` header. If no token is configured, every management request is rejected (403) — this group cannot run unauthenticated. |
| `/api/v1/ws`, `/api/v1/tasks/{id}/file` | `?token=<token>` query parameter (browser navigation/WebSocket upgrades can't set custom headers). |

Constant-time comparison is used everywhere a token is checked, to avoid timing side-channels.

## Takeover/aria2 vs. management API: different semantics

`POST /download`, `/download/batch`, and `aria2.addUri` all funnel into the same "external download" path. **On the desktop app**, this pops a confirmation dialog before anything downloads — the assumption is a browser extension or userscript on an untrusted page is asking on the user's behalf, so a human confirms it. **On the headless server**, there's no UI to show a confirmation dialog, so the same entry points create the task directly, identically to the management API.

`POST /api/v1/tasks` (management API) always creates the task directly, with no confirmation — on both hosts. It assumes the caller is an already-authenticated, trusted automation client, not an untrusted web page acting through a userscript.

In short: on the desktop app, takeover/aria2 endpoints ask first and the management API doesn't; on the headless server, nothing asks first, because there's nobody to ask.

## curl examples

Create a task directly (management API):

```bash
curl -X POST http://<host>:17800/api/v1/tasks \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{"url":"https://example.com/file.zip","segments":8}'
# -> {"taskId":"..."}
```

List tasks:

```bash
curl http://<host>:17800/api/v1/tasks \
  -H "Authorization: Bearer <token>"
```

Add a download via the aria2-compatible RPC (works with existing aria2-targeting userscripts/clients):

```bash
curl -X POST http://<host>:17800/jsonrpc \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "id": "1",
    "method": "aria2.addUri",
    "params": ["token:<token>", ["https://example.com/file.zip"]]
  }'
```

`CreateTaskRequest` accepts `url` (required), and optional `fileName`, `saveDir`, `segments`, `cookies`, `referrer`, `proxyUrl`, `userAgent`, `queueId`, `checksum` (`algo=hexhash`), and `headers` — all camelCase in the JSON body. The full schema is in the OpenAPI document below.

## Interactive documentation

- [`/api-docs`](/api-docs) on this site renders the full OpenAPI 3.1 spec (generated from the actual route handlers) with a try-it-out UI, for the routes common to both hosts.
- A running headless server also serves its own live, merged spec (core + server-specific extension routes) at `/api/v1/docs` (Scalar UI) and `/api/v1/openapi.json` (raw JSON) — always in sync with the exact build you're running.
