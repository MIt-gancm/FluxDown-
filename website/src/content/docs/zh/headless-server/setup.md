---
title: 服务器部署
description: 从源码构建并运行 headless FluxDown 服务器,了解全部环境变量并安全地对外暴露。
section: headless-server
order: 1
sourceHash: "0a2ec102ed41"
---

`fluxdown_server` 是 FluxDown 下载引擎的 headless 版本:没有 Flutter 界面,也没有 Rinf/FFI 层。它把同一套 Rust 引擎(HTTP/HTTPS、FTP、BitTorrent、HLS、DASH)通过 HTTP、WebSocket 和一个内置的 Web 界面暴露出来,因此你可以把它跑在 NAS、家庭服务器或 VPS 上,在浏览器里远程管理下载。

目前没有预编译的发行包,需要从工作区源码用 Cargo 自行构建。

## 构建与运行

服务器代码在 `native/server`(包名 `fluxdown_server`,可执行文件名 `fluxdown-server`)。在仓库根目录执行:

```bash
# 开发运行(debug 构建,默认监听 0.0.0.0:17800)
cargo run -p fluxdown_server

# 生产构建
cargo build --release -p fluxdown_server
# 产物路径:target/release/fluxdown-server(Windows 下为 fluxdown-server.exe)
```

进程本身是自包含的:它会打开自己的 SQLite(或 PostgreSQL)数据库、运行下载引擎并托管 Web 界面——不需要额外的数据库服务或反向代理就能跑起来。

## 构建 Web 前端

Web 界面是 `web/` 目录下独立的 SPA(React 19 + TanStack,用 [Bun](https://bun.sh) 构建)。服务器只负责托管静态文件,不会替你构建它。

```bash
cd web
bun install
bun run build      # 输出到 web/dist
```

用下文的 `FLUXDOWN_WEBROOT` 把服务器指向这个输出目录。跳过这一步服务器依然能正常响应 API/WebSocket 请求,但浏览器打开它什么都看不到(没有可回退的 `index.html`)。

<!-- TODO(screenshot): 终端里 `cargo run -p fluxdown_server` 首次运行打印 token 横幅的截图 -->

## 环境变量

全部配置在启动时从环境变量一次性读取,没有配置文件。

| 变量 | 默认值 | 说明 |
|---|---|---|
| `FLUXDOWN_BIND` | `0.0.0.0:17800` | HTTP/WebSocket 服务监听的 TCP 地址。 |
| `FLUXDOWN_DATA_DIR` | 平台自动探测(见下表) | 数据库文件与日志所在目录。 |
| `FLUXDOWN_DATABASE_URL` | 未设置——使用数据目录下的 SQLite 文件 | 显式连接串:`sqlite:/path/to/file.db` 或 `postgres://user:pass@host/db`。 |
| `FLUXDOWN_WEBROOT` | 可执行文件同级的 `./web` | Web 界面静态文件(`bun run build` 产物)所在目录;SPA 路由回退到 `index.html`。 |
| `FLUXDOWN_DEMO` | 未设置(关闭) | 真值(`1`/`true`/`yes`/`on`)开启演示模式:仅允许下载内置生成的 64 MiB 演示文件,适合公开演示。 |
| `FLUXDOWN_DEMO_URL` | 未设置(关闭) | 用指定 URL 覆盖演示模式的内置生成文件,仅该 URL 可下载。 |

未设置 `FLUXDOWN_DATA_DIR` 时,数据目录探测规则与桌面客户端一致:

| 平台 | 目录 |
|---|---|
| Windows(便携版) | 可执行文件同级目录 |
| Windows(安装版) | `%LOCALAPPDATA%\FluxDown\` |
| Linux | `$XDG_DATA_HOME/fluxdown/` |
| macOS | `~/Library/Application Support/fluxdown/` |

headless 部署几乎总是应该显式设置 `FLUXDOWN_DATA_DIR` 为一个固定、有备份的路径,而不是依赖自动探测。

```bash
FLUXDOWN_BIND=0.0.0.0:8080 \
FLUXDOWN_DATA_DIR=/srv/fluxdown/data \
FLUXDOWN_WEBROOT=/srv/fluxdown/web/dist \
./fluxdown-server
```

## 首次运行与获取访问令牌

headless 服务器的管理 API 恒开(与桌面客户端默认关闭、需手动开启不同)。首次启动时,若尚未存有 token,服务器会生成一个并持久化到数据库,同时**只打印这一次**到 stderr:

```
==============================================================
  FluxDown Server 首次运行,已生成管理 token:
    fxd_1a2b3c4d5e6f7890a1b2c3d4e5f67890
  用它登录 Web 界面 / 调用管理 API(Authorization: Bearer)。
==============================================================
```

务必立即保存这个 token——只有生成它的那次运行会打印出来。用它来:

- 登录 Web 界面(见[Web 界面](/docs/zh/headless-server/web-ui/))。
- 用 `Authorization: Bearer <token>` 鉴权管理 API 调用(见 [API 总览](/docs/zh/api/overview/))。

token 存储在服务器自己数据库的 `config` 表里,只要数据库文件(或 PostgreSQL 数据库)还在,重启后依然有效。

### 重置令牌

如果 token 丢失或怀疑已泄露,可以在 Web 界面(**设置 → 安全与访问 → 访问令牌 → 重新生成**)重置,或者用当前 token 鉴权后直接调用管理 API:

```bash
curl -X POST http://<host>:17800/api/v1/token/regenerate \
  -H "Authorization: Bearer <当前token>"
```

响应里的新 token 会附带说明:**必须重启服务器进程后才生效**——运行中的进程在此之前仍沿用内存里的旧 token。

## 数据库:SQLite 默认,PostgreSQL 可选

默认情况下服务器会在数据目录里打开一个 SQLite 文件,无需任何设置。如果部署多实例或对吞吐量有更高要求,可以改用 PostgreSQL:

```bash
FLUXDOWN_DATABASE_URL=postgres://fluxdown:password@localhost/fluxdown \
cargo run -p fluxdown_server
```

连接串的 scheme(`sqlite:` 还是 `postgres:`)决定后端,两者共用同一套 schema 与迁移逻辑。服务器自己的日志会掩掉 `FLUXDOWN_DATABASE_URL` 里的凭证段,但这个环境变量本身仍要当作敏感信息对待(避免留在 shell 历史或明文提交到进程管理器配置里)。

## 安全地对外暴露(反向代理与 TLS)

`FLUXDOWN_BIND` 默认是 `0.0.0.0:17800`——监听所有网络接口,这与桌面客户端本机 API 硬编码只绑 `127.0.0.1` 不同。这是 headless 场景的刻意设计,但意味着**网络边界的安全由你负责**:

- 管理 token 是互联网与"完全远程控制你的服务器"(创建/删除下载、通过目录选择器浏览服务器文件系统、取回任意已完成文件)之间唯一的屏障。把它当 root 密码对待:不要分享、不要打进日志,一旦怀疑泄露就重新生成。
- 如果服务器需要在可信局域网之外访问,把它放在反向代理(nginx、Caddy、Traefik)之后终结 TLS,只对外暴露 HTTPS。Web 界面登录时 token 会出现在请求体/查询字符串里,明文 HTTP 下会被网络路径上的任何人看到。
- WebSocket 端点(`/api/v1/ws`)需要代理转发 `Upgrade`/`Connection` 头。最简 nginx 片段:

  ```nginx
  location / {
      proxy_pass http://127.0.0.1:17800;
      proxy_http_version 1.1;
      proxy_set_header Upgrade $http_upgrade;
      proxy_set_header Connection "upgrade";
      proxy_set_header Host $host;
  }
  ```

- 相比直接把端口暴露给公网(即使配了 TLS),更推荐绑定到私有接口(`FLUXDOWN_BIND=127.0.0.1:17800`,由反向代理挡在前面)或 VPN/Tailscale 地址。

## 作为 systemd 服务运行

Linux 部署的最小 unit 文件示例(按需调整路径与用户):

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

把 `fluxdown-server`(release 二进制)与构建好的 `web/dist` 内容(重命名为 `web/`)放到 `/opt/fluxdown` 下,创建 `fluxdown` 系统用户与 `/var/lib/fluxdown` 目录,然后:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now fluxdown-server
sudo journalctl -u fluxdown-server -f   # 观察首次运行打印的 token 横幅
```

## 下一步

- [Web 界面](/docs/zh/headless-server/web-ui/)——在浏览器里登录并管理下载。
- [API 总览](/docs/zh/api/overview/)——用脚本或其它工具自动化操作服务器。
