/**
 * 阿里云 OSS 预签名（V1 query 签名）。
 *
 * 发布资产由 .github/actions/oss-upload 上传到 `oss://<bucket>/<prefix>/<tag>/<file>`，
 * bucket 保持私有（对象不可公共读），官网按需签出短期 URL 后 302。
 * 签名算法：https://help.aliyun.com/zh/oss/developer-reference/signature-version-1
 */

import { createHmac } from "node:crypto";
import {
  OSS_ACCESS_KEY_ID,
  OSS_ACCESS_KEY_SECRET,
  OSS_BUCKET,
  OSS_ENDPOINT,
  OSS_RELEASE_PREFIX,
} from "astro:env/server";

/** 未配置 AK/SK 时整条 OSS 路径关闭（下载路由回退 GitHub）。 */
export const ossConfigured = !!(OSS_ACCESS_KEY_ID && OSS_ACCESS_KEY_SECRET);

/**
 * 发布资产的对象键（无前导斜杠）：`<prefix>/<版本>/<组件>/<file>`。
 * 组件 tag 规则与 release.yml 一致：`v0.4.8` → app，`extension-v0.4.8` /
 * `server-v…` / `cli-v…` / `mobile-v…` → 同名组件；版本目录保留 `v` 前缀。
 * 同一套规则在 .github/actions/oss-upload/action.yml 的 bash 里复刻，改一处须同步另一处。
 */
export function releaseObjectKey(tag: string, filename: string): string {
  const m = /^(?:([a-z]+)-)?(v.+)$/.exec(tag);
  const component = m?.[1] ?? "app";
  const version = m?.[2] ?? tag;
  return `${OSS_RELEASE_PREFIX}/${version}/${component}/${filename}`;
}

/**
 * 生成 `ttlSec` 秒内有效的预签名 URL。签名串用原始 key，URL 路径按段编码
 * （OSS 用解码后的资源路径校验签名）。调用前须保证 `ossConfigured`。
 */
export function presignOssUrl(
  method: "GET" | "HEAD",
  key: string,
  ttlSec: number,
): string {
  const expires = Math.floor(Date.now() / 1000) + ttlSec;
  const stringToSign = `${method}\n\n\n${expires}\n/${OSS_BUCKET}/${key}`;
  const signature = createHmac("sha1", OSS_ACCESS_KEY_SECRET ?? "")
    .update(stringToSign)
    .digest("base64");
  const path = key.split("/").map(encodeURIComponent).join("/");
  const query = new URLSearchParams({
    OSSAccessKeyId: OSS_ACCESS_KEY_ID ?? "",
    Expires: String(expires),
    Signature: signature,
  });
  return `https://${OSS_BUCKET}.${OSS_ENDPOINT}/${path}?${query}`;
}
