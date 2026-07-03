---
title: API 总览
description: FluxDown 的 HTTP API——四组路由、鉴权方式,以及它与 headless 服务器的关系。
section: api
order: 1
sourceHash: "2de9321e528c"
---

FluxDown 内置一套小型 HTTP API,供浏览器扩展、油猴脚本、aria2 客户端与自动化工具使用,存在于两个地方:

- **桌面客户端**,地址 `http://127.0.0.1:17800`(端口可配置,地址硬编码为回环,永远不会暴露在网络上)。管理 API 分组默认关闭,其余分组默认开启;具体见桌面客户端的本机 API 设置。
- **[headless 服务器](/docs/zh/headless-server/setup/)**,地址取决于 `FLUXDOWN_BIND` 的设置(默认 `0.0.0.0:17800`——刻意监听网络接口,因为远程管理正是它存在的意义)。管理 API 在这里恒开,并且在桌面客户端已有的端点之外额外挂载了几个 headless 专属端点(队列、配置、文件取回、WebSocket、服务器文件系统浏览)。

两者共用同一套路由常量、请求/响应 JSON 契约与鉴权规则——区别只在于哪些路由组被启用,以及由哪个宿主实现。

## 四组路由

| 分组 | 端点 | 开关 | 鉴权 |
|---|---|---|---|
| 探活 | `GET /ping` | 总开关 | 无 |
| 脚本接管 | `POST /download`、`POST /download/batch` | `local_server_takeover_enabled`(默认开) | 必须带 `X-FluxDown-Client` 头,外加可选 token |
| aria2 兼容 RPC | `POST /jsonrpc`(`aria2.addUri`、`aria2.getVersion`、`aria2.getGlobalStat`、`system.multicall`、`system.listMethods`) | `local_server_jsonrpc_enabled`(默认开) | 可选 token |
| 管理 API | `GET /api/v1/info`、`GET/POST /api/v1/tasks`、`GET/DELETE /api/v1/tasks/{id}`、`PUT /api/v1/tasks/{id}/pause\|continue`、`PUT /api/v1/tasks/pause\|continue`、`GET /api/v1/queues` | `local_server_api_enabled`(桌面默认关,headless 服务器恒开) | **强制** token |

管理分组开启时,`GET /api/v1/openapi.json`(无鉴权,纯接口描述不含数据)始终可用。

headless 服务器额外把这些端点挂在 `/api/v1/*` 下,是桌面客户端没有的:`GET /api/v1/ws`(WebSocket)、`GET/PUT /api/v1/config`、`POST/PUT/DELETE /api/v1/queues[/{id}]`、`PUT /api/v1/tasks/{id}/queue`、`PUT /api/v1/tasks/{id}/boost`、`GET /api/v1/tasks/{id}/file`、`GET /api/v1/fs/list`、`POST /api/v1/proxy/test`、`POST /api/v1/token/regenerate`、`GET /api/v1/stats`。这些端点遵循与管理 API 相同的鉴权规则,例外是 `/ws` 与 `/tasks/{id}/file`(浏览器发起的请求无法自定义请求头,改用 `?token=` 查询参数)以及 `/openapi.json`/`/docs`(无鉴权)。

## 鉴权方式

服务器只配置一个 token(`local_server_token`);具体怎么传取决于路由组:

| 路由组 | 接受的形式 |
|---|---|
| 脚本接管 | `X-FluxDown-Token` 头(仅在配置了 token 时才校验;token 为空即该分组不鉴权)。无论是否配置 token,都必须带 `X-FluxDown-Client` 头——靠 CORS 挡住任意网页脚本的门禁。 |
| aria2 兼容 RPC | `X-FluxDown-Token` 头,**或** aria2 自己的约定——在 JSON-RPC 调用的 `params[0]` 里传 `token:xxx`。 |
| 管理 API(`/api/v1/*`) | `Authorization: Bearer <token>` **或** `X-FluxDown-Token` 头。未配置 token 时该分组的一切请求都会被拒绝(403)——这组端点不能在无鉴权状态下运行。 |
| `/api/v1/ws`、`/api/v1/tasks/{id}/file` | `?token=<token>` 查询参数(浏览器的导航跳转/WebSocket 升级无法自定义请求头)。 |

所有 token 校验都使用常量时间比较,避免时序侧信道。

## 接管 / aria2 与管理 API 的语义区别

`POST /download`、`/download/batch` 与 `aria2.addUri` 都汇入同一条"外部下载"通道。**在桌面客户端上**,这条通道会在真正下载前弹出确认框——前提假设是某个浏览器扩展或不可信网页上的油猴脚本在替用户发起请求,所以需要人工确认。**在 headless 服务器上**没有界面可以弹确认框,同样的入口会直接创建任务,与管理 API 行为一致。

`POST /api/v1/tasks`(管理 API)在两种宿主上都是**直接创建任务、不弹确认框**——它假设调用方是已经通过鉴权的可信自动化客户端,而不是经由油猴脚本代为发起的不可信网页。

简单说:桌面客户端上接管/aria2 入口会先问,管理 API 不问;headless 服务器上没人能问,所以都不问。

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

`CreateTaskRequest` 接受必填的 `url`,以及可选的 `fileName`、`saveDir`、`segments`、`cookies`、`referrer`、`proxyUrl`、`userAgent`、`queueId`、`checksum`(`algo=hexhash`)与 `headers`——JSON body 全部为 camelCase。完整字段定义见下方的 OpenAPI 文档。

## 交互式文档

- 本站的 [`/api-docs`](/api-docs) 渲染完整的 OpenAPI 3.1 规范(由真实路由 handler 生成),带在线试调用界面,覆盖两种宿主共有的路由。
- 运行中的 headless 服务器还会自己提供实时的合并版规范(核心路由 + 服务器专属扩展路由):`/api/v1/docs`(Scalar 界面)与 `/api/v1/openapi.json`(原始 JSON)——始终与你正在运行的那个版本保持一致。
