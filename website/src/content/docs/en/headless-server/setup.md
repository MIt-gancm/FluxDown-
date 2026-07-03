---
title: Server Setup
description: Build and run the headless FluxDown server, configure it with environment variables, and expose it safely.
section: headless-server
order: 1
---

`fluxdown_server` is a headless build of the FluxDown download engine: no Flutter UI, no Rinf/FFI layer. It exposes the same Rust engine (HTTP/HTTPS, FTP, BitTorrent, HLS, DASH) over HTTP, WebSocket, and a bundled Web UI, so you can run it on a NAS, a home server, or a VPS and manage downloads remotely from a browser.

There is currently no prebuilt binary release — you build it from the workspace source with Cargo.

## Build and run

The server lives in the `native/server` crate (package name `fluxdown_server`, binary name `fluxdown-server`). From the repository root:

```bash
# Development run (debug build, default bind 0.0.0.0:17800)
cargo run -p fluxdown_server

# Production build
cargo build --release -p fluxdown_server
# binary at target/release/fluxdown-server (fluxdown-server.exe on Windows)
```

The process is self-contained: it opens its own SQLite (or PostgreSQL) database, runs the download engine, and serves the Web UI — no separate database server or reverse proxy is required to get started.

## Building the web front end

The Web UI is a separate SPA in `web/` (React 19 + TanStack, built with [Bun](https://bun.sh)). The server only serves static files — it does not build them for you.

```bash
cd web
bun install
bun run build      # outputs to web/dist
```

Point the server at that output directory with `FLUXDOWN_WEBROOT` (see below). If you skip this step, the server still runs and answers API/WebSocket requests, but visiting it in a browser serves nothing (no `index.html` to fall back to).

<!-- TODO(screenshot): terminal showing `cargo run -p fluxdown_server` output with the first-run token banner -->

## Environment variables

All configuration is read once at startup from environment variables. There is no config file.

| Variable | Default | Description |
|---|---|---|
| `FLUXDOWN_BIND` | `0.0.0.0:17800` | TCP address the HTTP/WebSocket server listens on. |
| `FLUXDOWN_DATA_DIR` | Platform auto-detected (see below) | Directory holding the database file and logs. |
| `FLUXDOWN_DATABASE_URL` | unset — uses a SQLite file inside the data dir | Explicit connection string: `sqlite:/path/to/file.db` or `postgres://user:pass@host/db`. |
| `FLUXDOWN_WEBROOT` | `./web` next to the executable | Directory the Web UI static files (`bun run build` output) are served from; SPA routes fall back to `index.html`. |
| `FLUXDOWN_DEMO` | unset (off) | Truthy value (`1`/`true`/`yes`/`on`) turns on demo mode: only a built-in, generated 64 MiB file can be downloaded. Useful for public demos. |
| `FLUXDOWN_DEMO_URL` | unset (off) | Overrides demo mode's allowed URL with a specific one instead of the built-in generated file. |

When `FLUXDOWN_DATA_DIR` is not set, the data directory is auto-detected the same way the desktop app does:

| Platform | Directory |
|---|---|
| Windows (portable build) | next to the executable |
| Windows (installed) | `%LOCALAPPDATA%\FluxDown\` |
| Linux | `$XDG_DATA_HOME/fluxdown/` |
| macOS | `~/Library/Application Support/fluxdown/` |

For a headless deployment you almost always want to set `FLUXDOWN_DATA_DIR` explicitly to a stable, backed-up path instead of relying on auto-detection.

```bash
FLUXDOWN_BIND=0.0.0.0:8080 \
FLUXDOWN_DATA_DIR=/srv/fluxdown/data \
FLUXDOWN_WEBROOT=/srv/fluxdown/web/dist \
./fluxdown-server
```

## First run and the access token

The management API is always enabled on the headless server (unlike the desktop app, where it is opt-in). On first boot, if no token is stored yet, the server generates one, persists it to the database, and prints it once to **stderr**:

```
==============================================================
  FluxDown Server 首次运行，已生成管理 token：
    fxd_1a2b3c4d5e6f7890a1b2c3d4e5f67890
  用它登录 Web 界面 / 调用管理 API（Authorization: Bearer）。
==============================================================
```

Capture that token immediately — it is only printed on the run that generates it. Use it to:

- Sign in to the Web UI (see [Web UI](/docs/en/headless-server/web-ui/)).
- Authenticate management API calls with `Authorization: Bearer <token>` (see [API Overview](/docs/en/api/overview/)).

The token is stored in the `config` table of the server's own database, so it survives restarts as long as the database file (or PostgreSQL database) persists.

### Resetting the token

If you lose the token or suspect it leaked, regenerate it from the Web UI (**Settings → Security & Access → Access Token → regenerate**) or by calling the management API itself while authenticated with the current token:

```bash
curl -X POST http://<host>:17800/api/v1/token/regenerate \
  -H "Authorization: Bearer <current-token>"
```

The response includes the new token and a note that it only takes effect **after the server process restarts** — the running process keeps using the old token in memory until then.

## Database: SQLite and PostgreSQL

By default the server opens a SQLite database file inside the data directory — no setup needed. For multi-instance or higher-throughput deployments, point it at PostgreSQL instead:

```bash
FLUXDOWN_DATABASE_URL=postgres://fluxdown:password@localhost/fluxdown \
cargo run -p fluxdown_server
```

The connection string's scheme (`sqlite:` vs `postgres:`) selects the backend; both share the same schema and migrations. Credentials in `FLUXDOWN_DATABASE_URL` are masked in the server's own log output, but treat the environment variable itself like any other secret (avoid putting it in shell history or committing it to a process manager's config in plaintext where avoidable).

## Exposing it safely (reverse proxy & TLS)

`FLUXDOWN_BIND` defaults to `0.0.0.0:17800` — reachable on every network interface, unlike the desktop app's local API which is hardcoded to loopback only. That is intentional for headless use, but it means **you** are responsible for the network boundary:

- The management token is the only thing standing between the internet and full remote control of your server (create/delete downloads, browse the server's filesystem via the directory picker, stream any completed file back). Treat it like a root password: don't share it, don't log it, rotate it if it may have leaked.
- If the server is reachable beyond a trusted LAN, put it behind a reverse proxy (nginx, Caddy, Traefik) terminating TLS, and only expose HTTPS. The Web UI's login screen sends the token in a request body/query string; on plain HTTP that is visible to anyone on the network path.
- The WebSocket endpoint (`/api/v1/ws`) needs `Upgrade`/`Connection` headers forwarded by the proxy. A minimal nginx snippet:

  ```nginx
  location / {
      proxy_pass http://127.0.0.1:17800;
      proxy_http_version 1.1;
      proxy_set_header Upgrade $http_upgrade;
      proxy_set_header Connection "upgrade";
      proxy_set_header Host $host;
  }
  ```

- Prefer binding to a private interface (`FLUXDOWN_BIND=127.0.0.1:17800` and letting the reverse proxy sit in front, or a VPN/Tailscale address) over exposing the port directly to the public internet, even with TLS.

## Running as a systemd service

A minimal unit file for a Linux deployment (adjust paths and user):

```ini
[Unit]
Description=FluxDown headless download server
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=fluxdown
Group=fluxdown
WorkingDirectory=/opt/fluxdown
Environment=FLUXDOWN_BIND=0.0.0.0:17800
Environment=FLUXDOWN_DATA_DIR=/var/lib/fluxdown
Environment=FLUXDOWN_WEBROOT=/opt/fluxdown/web
ExecStart=/opt/fluxdown/fluxdown-server
Restart=on-failure
RestartSec=5
NoNewPrivileges=true

[Install]
WantedBy=multi-user.target
```

Install `fluxdown-server` (the release binary) and the built `web/dist` contents (renamed to `web/`) under `/opt/fluxdown`, create the `fluxdown` system user and `/var/lib/fluxdown`, then:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now fluxdown-server
sudo journalctl -u fluxdown-server -f   # watch for the first-run token banner
```

## Next steps

- [Web UI](/docs/en/headless-server/web-ui/) — sign in and manage downloads from a browser.
- [API Overview](/docs/en/api/overview/) — automate the server from scripts or other tools.
