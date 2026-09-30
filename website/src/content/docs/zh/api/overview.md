---
title: API 总览
description: FluxDown 的 HTTP API——五组路由、鉴权方式,以及它与 headless 服务器的关系。
section: api
order: 1
sourceHash: "618afb3e9283"
---

FluxDown 内置一套小型 HTTP API,供浏览器扩展、油猴脚本、aria2 客户端与自动化工具使用,存在于两个地方:

- **桌面客户端**,地址 `http://127.0.0.1:17800`(端口可配置,地址硬编码为回环,永远不会暴露在网络上)。管理 API 分组默认关闭,其余分组默认开启;具体见桌面客户端的本机 API 设置。
- **[headless 服务器](/docs/zh/headless-server/setup/)**,地址取决于 `FLUXDOWN_BIND` 的设置(默认 `0.0.0.0:17800`——刻意监听网络接口,因为远程管理正是它存在的意义)。服务器即 `fluxdown-agent --server`(连同同级的 `fluxdownd`);下表的兼容分组从首次启动就开启。其余管理功能由内置 Web 界面经 agent 的 `/rpc` JSON-RPC 端点完成——与桌面客户端同一协议。

两者共用同一套路由常量、请求/响应 JSON 契约与鉴权规则——区别只在于哪些路由组被启用,以及由哪个宿主实现。

## 五组路由

| 分组 | 端点 | 开关 | 鉴权 |
|---|---|---|---|
| 探活 | `GET /ping` | 总开关 | 无 |
| 脚本接管 | `POST /download`、`POST /download/batch` | `local_server_takeover_enabled`(默认开) | 必须带 `X-FluxDown-Client` 头,外加可选 token |
| aria2 兼容 RPC | `POST /jsonrpc`(`aria2.addUri`、`aria2.getVersion`、`aria2.getGlobalStat`、`system.multicall`、`system.listMethods`) | `local_server_jsonrpc_enabled`(默认开) | 可选 token |
| 管理 API | `GET /api/v1/info`、`GET/POST /api/v1/tasks`、`GET/DELETE /api/v1/tasks/{id}`、`PUT /api/v1/tasks/{id}/pause\|continue`、`PUT /api/v1/tasks/pause\|continue`、`GET /api/v1/queues` | `local_server_api_enabled`(桌面默认关,headless 服务器恒开) | **强制** token |
| MCP | `POST /mcp`(`initialize`、`tools/list`、`tools/call`、`ping`) | `local_server_mcp_enabled`(桌面默认关,headless 服务器恒开) | **强制** token(与管理 API 共用) |

管理分组开启时,`GET /api/v1/openapi.json`(无鉴权,纯接口描述不含数据)始终可用。

旧版 headless 服务器(`fluxdown-server`)还额外提供过一批扩展 REST 路由——`/api/v1/config`、队列增删改与 start/stop/schedule/order、`/api/v1/stats`、`/api/v1/fs/list`、组件、webhook、日志、`/api/v1/ws`、`/api/v1/token/regenerate`。**在 `fluxdown-agent --server` 上它们已不存在**:请用内置 Web 界面管理这些内容,或直接对 `/rpc` 说 JSON-RPC。服务器另外提供两个免鉴权的引导路由 `GET /api/v1/setup/status` 与 `POST /api/v1/setup`(仅在尚未设置访问密钥时接受),供首次运行向导使用。

## 鉴权方式

服务器只配置一个 token(`local_server_token`);具体怎么传取决于路由组:

| 路由组 | 接受的形式 |
|---|---|
| 脚本接管 | `X-FluxDown-Token` 头(仅在配置了 token 时才校验;token 为空即该分组不鉴权)。无论是否配置 token,都必须带 `X-FluxDown-Client` 头——靠 CORS 挡住任意网页脚本的门禁。 |
| aria2 兼容 RPC | `X-FluxDown-Token` 头,**或** aria2 自己的约定——在 JSON-RPC 调用的 `params[0]` 里传 `token:xxx`。 |
| 管理 API(`/api/v1/*`) | `Authorization: Bearer <token>` **或** `X-FluxDown-Token` 头。未配置 token 时该分组的一切请求都会被拒绝(403)——这组端点不能在无鉴权状态下运行。 |
| `/rpc`(WebSocket) | `Authorization: Bearer <token>`;浏览器则用 `Sec-WebSocket-Protocol: fluxdown.rpc.v1, fluxdown.token.<base64url(token)>` 子协议。带 `Origin` 头的请求必须与 `Host` 同源。 |
| `/api/web/files/tasks/{id}`、`/api/web/exports/{id}` | `Authorization: Bearer <token>` 或 `?token=<token>` 查询参数(浏览器的导航跳转无法自定义请求头)。 |

所有 token 校验都使用常量时间比较,避免时序侧信道。

### 跨域(CORS)

服务默认对任何请求都**不返回** `Access-Control-Allow-Origin`,带自定义头的跨域 `fetch()`(如 `/download*` 要求的 `X-FluxDown-Client`)会在预检阶段被浏览器拦下(油猴脚本走 `GM_xmlhttpRequest`,不受 CORS 约束)。但 `/jsonrpc` 的 `text/plain` POST 与 WebSocket 不触发预检,因此服务端还会校验 `Origin`:在 `/jsonrpc`(POST 与 WS 升级)和 `/download*` 上,带 `Origin` 头的请求只放行浏览器扩展(`chrome-extension://`、`moz-extension://`、`safari-web-extension://`)与同源请求,其余返回 403;不带 `Origin` 的非浏览器客户端(油猴 `GM_xmlhttpRequest`、CLI、aria2 客户端)不受影响。桌面端仅回环监听时还会要求 `Host` 为 `127.0.0.1` / `localhost` / `[::1]`,防止 DNS 重绑定。

设置里的**允许任意网页跨域访问(CORS)**(`local_server_cors_allow_all`,默认关)可以放弃这道防线:开启后预检与真实响应都带 `Access-Control-Allow-Origin: *`,预检额外带 `Access-Control-Allow-Private-Network: true`,等价于 aria2 的 `--rpc-allow-origin-all`,同时 `Origin` 门禁也随之放开。用途是让那些"用浏览器 `fetch` 探测 aria2 服务"的网站能识别到 FluxDown;代价是任意网页都能探测本机端口并提交下载链接。此时仍生效的防护:管理 API 与 MCP 仍强制校验 token;aria2 入口**不会**弹确认框,建议同时设置 token(token 为空时 `aria2.changeGlobalOption` 会拒绝修改 `dir`)。

## 接管 / aria2 与管理 API 的语义区别

`POST /download`、`/download/batch` 汇入"外部下载"通道:**在桌面客户端上**会在真正下载前弹出确认框(用户开启免打扰下载后则静默建任务);**在 headless 服务器上**没有界面可以弹确认框,会直接创建任务。`aria2.addUri` / `aria2.addTorrent` **不经过确认通道**,在所有宿主上都直接创建任务,与管理 API 一致——它们的防护来自 `Origin` 门禁与可选 token,而不是确认框。

`POST /api/v1/tasks`(管理 API)在两种宿主上都是**直接创建任务、不弹确认框**——它假设调用方是已经通过鉴权的可信自动化客户端,而不是经由油猴脚本代为发起的不可信网页。

简单说:只有脚本接管入口在桌面客户端上会先问;aria2 与管理 API 都不问。请给 aria2 入口配置 token(尤其在开启 CORS 或局域网访问时)。

## curl 示例

直接创建任务(管理 API):

```bash
curl -X POST http://<host>:17800/api/v1/tasks \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{"url":"https://example.com/file.zip","segments":8}'
# -> {"taskId":"..."}
```

查询任务列表:

```bash
curl http://<host>:17800/api/v1/tasks \
  -H "Authorization: Bearer <token>"
```

用 aria2 兼容 RPC 添加下载(现成的面向 aria2 的油猴脚本/客户端可直接工作):

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

`CreateTaskRequest` 接受必填的 `url`,以及可选的 `fileName`、`saveDir`、`segments`、`cookies`、`referrer`、`proxyUrl`、`userAgent`、`queueId`、`checksum`(`algo=hexhash`)与 `headers`——JSON body 全部为 camelCase。传入的 `fileName` 会被清洗(剥除路径分隔符与 `..`),确保下载始终落在其保存目录内。完整字段定义见下方的 OpenAPI 文档。

## MCP(Model Context Protocol)

FluxDown 支持通过 HTTP 提供 [MCP](https://modelcontextprotocol.io) 服务,让 AI 客户端(Claude Desktop、Cursor、Cline 及任何支持 MCP 的智能体)用自然语言驱动下载。它是单个端点 `POST /mcp`,由与管理 API 相同的 token 保护。

MCP 是"JSON-RPC 2.0 over 单 HTTP 端点"(不是 REST)——每个操作都是一次 POST 到 `/mcp`,靠请求体里的 `method` 区分,采用 Streamable HTTP 传输的无状态子集:请求返回 `application/json`,通知返回 `202 Accepted`,不跟踪会话 id。用 `Authorization: Bearer <token>`(或 `X-FluxDown-Token`)鉴权;规范允许内部部署用静态 bearer token 代替 OAuth 2.1。

### 工具列表

客户端先调 `tools/list` 在运行时发现这些工具(每个都自带完整的参数 JSON Schema),再用 `tools/call` 调用其中之一:

| 工具 | 作用 | 参数 |
|---|---|---|
| `download_add` | 新建下载任务(HTTP/HTTPS/FTP/磁力/BitTorrent)。返回新任务 id。 | `url`(必填);可选 `fileName`、`saveDir`、`segments`、`proxyUrl`、`cookies`、`referrer`、`userAgent`、`queueId`、`checksum` |
| `download_list` | 列出任务,可按状态过滤。 | `status`(可选:`all`/`pending`/`downloading`/`paused`/`completed`/`error`/`preparing`) |
| `download_get` | 按 id 查询单个任务的完整详情。 | `taskId`(必填) |
| `download_pause` | 暂停指定任务。 | `taskId`(必填) |
| `download_resume` | 恢复指定的已暂停任务。 | `taskId`(必填) |
| `download_pause_all` | 暂停全部活跃任务(pending / downloading / preparing)。 | 无 |
| `download_resume_all` | 恢复全部已暂停任务。 | 无 |
| `download_remove` | 删除任务,可选同时删除磁盘文件。 | `taskId`(必填);可选 `deleteFiles`(布尔) |
| `queue_list` | 列出全部命名队列及其配置。 | 无 |

这九个工具全部直接映射到管理 API 的宿主能力,所以 MCP 客户端与 REST 客户端看到的是完全相同的任务与队列。

### 接入客户端

把 MCP 客户端指向该端点并带上 bearer token,例如在 `mcp.json` 里:

```json
{
  "mcpServers": {
    "fluxdown": {
      "url": "http://<host>:17800/mcp",
      "headers": { "Authorization": "Bearer <token>" }
    }
  }
}
```

或直接用 curl 试调用——先 initialize,再调一个工具:

```bash
curl -X POST http://<host>:17800/mcp \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/call",
       "params":{"name":"download_add",
                 "arguments":{"url":"https://example.com/file.zip","segments":8}}}'
```

## fluxdown:// URL 协议

在 HTTP API 之外,FluxDown 还注册了一个自定义 URL 协议,任何网页、脚本或第三方应用都可以用它转交下载——不需要发起本机 HTTP 调用:

```text
fluxdown://download?url=<percent 编码的 URL>&filename=<可选文件名>
```

- `url`——必填。要下载的地址,需 percent 编码(`http`/`https`/`ftp` 直链或 `magnet:` 链接)。缺少或为空 `url` 参数的 `fluxdown://` URL 会被静默忽略。
- `filename`——可选。建议文件名,会预填给用户保留或修改。当真实文件名只存在于接收方永远看不到的 `Content-Disposition` 响应头里时特别有用。

由谁响应取决于平台:

- **桌面端(Windows、macOS、Linux)**——客户端注册系统协议处理器(Windows 每次启动写注册表;macOS 经 `CFBundleURLTypes` 声明;Linux 经 `.desktop` 文件的 `x-scheme-handler` 条目)。打开 `fluxdown://` URL 会启动客户端(或转发给已在运行的实例),并把请求路由进与浏览器扩展请求相同的外部下载流程:默认弹快速下载确认框,用户开启免打扰下载后则静默建任务。在 Android 以及受限的桌面环境中,浏览器扩展本身也可以经此协议投递——见 [fluxdown:// 协议模式](/docs/zh/browser-extension/usage/)。
- **Android**——应用为该 scheme 声明了 VIEW intent-filter。打开 URL 会唤起应用并显示新建下载弹层,`url` 与 `filename` 已预填;用户确认后才开始下载。弹层打开期间陆续到达的协议 URL 会作为新行合入其中(浏览器扩展在 Android 上就是这样投递批量下载的)。

一个普通的 HTML 链接就能完成集成:

```html
<a href="fluxdown://download?url=https%3A%2F%2Fexample.com%2Ffile.zip&filename=file.zip">
  用 FluxDown 下载
</a>
```

注意该协议不携带任何 Cookie、请求头或凭据——接收方会从零发起对该 URL 的请求。需要认证的下载请改用上面的脚本接管或管理 API 端点,它们的请求体接受 `cookies` 与 `headers`。

## 交互式文档

- 本站的 [`/api-docs`](/api-docs) 渲染完整的 OpenAPI 3.1 规范(由真实路由 handler 生成),带在线试调用界面,覆盖两种宿主共有的路由。
- 运行中的服务器还会自己提供实时规范:`/api/v1/openapi.json`(原始 JSON)——始终与你正在运行的那个版本保持一致。
