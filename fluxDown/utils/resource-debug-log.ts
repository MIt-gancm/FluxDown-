/**
 * Resource sniffer diagnostics export.
 *
 * The export keeps URL paths and parsed media metadata, because those are the
 * facts needed to diagnose aggregation. Credentials in query strings and in
 * path-embedded signed tokens (Akamai-style `hdnts=exp=...~hmac=...` segments,
 * `/token/<jwt>/...` segments) are redacted, and cookies/header values are
 * never serialized.
 */

import type { DashManifest } from "./dash-manifest";
import type {
  DashManifestEntry,
  MediaCandidate,
  MediaCandidateVariant,
} from "./media-candidates";
import { countMediaCandidateRows } from "./media-candidates";
import type { DetectedResource } from "./resource-types";
import { normalizeUrlForDedup } from "./resource-types";

export interface ResourceDebugLogOptions {
  resources: DetectedResource[];
  manifests?: DashManifestEntry[];
  candidates: MediaCandidate[];
  tabId?: number;
  pageUrl?: string;
  pageTitle?: string;
  source: "popup" | "content";
}

const SENSITIVE_QUERY_KEYS = new Set([
  "token",
  "access_token",
  "auth",
  "authorization",
  "sign",
  "signature",
  "sig",
  "policy",
  "expires",
  "expires_at",
  "expire",
  "key-pair-id",
  "key_pair_id",
  "x-amz-signature",
  "x-amz-credential",
  "x-amz-security-token",
  "hdnts",
]);

function isSensitiveQueryKey(key: string): boolean {
  // hmac/sig：Akamai `hdnts=exp=…~acl=/*~hmac=…` 的 acl 若含 `/`，URL 解析会把
  // `hmac=…` 拆成独立路径段，此时子键本身必须能被识别，否则签名原样落盘。
  return SENSITIVE_QUERY_KEYS.has(key.toLowerCase()) ||
    /(^|[-_])(token|sign|sig|signature|hmac|credential|authorization|auth|key.?pair.?id|policy|expires?|access.?token)([-_]|$)/i.test(key);
}

/**
 * A base64/hex-style opaque path segment (bearer token, signed cookie, JWT
 * part) rather than a human filename: long, no filename-style separators
 * (space/underscore), and either pure hex or mixes case + digits the way
 * encoded binary does — natural titles this long are near-universally
 * lowercase-with-separators or carry a recognizable extension.
 */
function looksLikeOpaqueToken(segment: string): boolean {
  if (segment.length < 32 || /[\s_]/.test(segment)) return false;
  if (!/^[A-Za-z0-9+/-]+$/.test(segment)) return false;
  if (/^[0-9a-fA-F]+$/.test(segment)) return true;
  return /[0-9]/.test(segment) && /[a-z]/.test(segment) && /[A-Z]/.test(segment);
}

/**
 * Redact a signed-token path segment. Two shapes seen in the wild:
 * - Akamai-style `key=value[~key=value...]` blob (`hdnts=exp=...~hmac=...`).
 *   Any sensitive sub-key (leading `hdnts=` or inner `hmac=`) redacts the whole
 *   blob: the acl sub-value may contain `/`, in which case URL parsing splits
 *   the blob across several path segments and the `hmac=...` tail lands in a
 *   segment of its own, so the leading key alone cannot be relied upon.
 * - A bare JWT (three dot-joined base64url parts) or a long base64/hex run
 *   embedded directly in the path, e.g. `/token/<jwt>/seg.m4s`.
 */
function redactPathSegment(segment: string): string {
  if (!segment) return segment;
  if (segment.includes("=")) {
    const sensitive = segment.split("~").some((part) => {
      const eq = part.indexOf("=");
      return eq > 0 && isSensitiveQueryKey(part.slice(0, eq).replace(/^[^A-Za-z0-9]+/, ""));
    });
    return sensitive ? "[REDACTED]" : segment;
  }
  if (/^[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}$/.test(segment)) {
    return "[REDACTED]";
  }
  return looksLikeOpaqueToken(segment) ? "[REDACTED]" : segment;
}

export function redactUrl(url: string | undefined): string {
  if (!url) return "";
  try {
    const parsed = new URL(url);
    for (const key of [...parsed.searchParams.keys()]) {
      if (isSensitiveQueryKey(key)) parsed.searchParams.set(key, "[REDACTED]");
    }
    parsed.pathname = parsed.pathname.split("/").map(redactPathSegment).join("/");
    return parsed.toString();
  } catch {
    return url;
  }
}

function canonicalUrl(url: string | undefined): string {
  return url ? normalizeUrlForDedup(redactUrl(url)) : "";
}

function timestampIso(value: number): string | undefined {
  if (!Number.isFinite(value) || value <= 0) return undefined;
  try {
    return new Date(value).toISOString();
  } catch {
    return undefined;
  }
}

function debugTrack(track: DashManifest["video"][number]) {
  return {
    id: track.id,
    url: redactUrl(track.url),
    canonicalUrl: canonicalUrl(track.url),
    mimeType: track.mimeType,
    codecs: track.codecs,
    bandwidth: track.bandwidth,
    width: track.width,
    height: track.height,
    downloadable: track.downloadable !== false,
  };
}

function debugVariant(variant: MediaCandidateVariant) {
  return {
    id: variant.id,
    label: variant.label,
    videoUrl: redactUrl(variant.videoUrl),
    canonicalVideoUrl: canonicalUrl(variant.videoUrl),
    audioUrl: redactUrl(variant.audioUrl),
    canonicalAudioUrl: canonicalUrl(variant.audioUrl),
    mimeType: variant.mimeType,
    bandwidth: variant.bandwidth,
    codec: variant.codec,
    fileSize: variant.fileSize,
    resourceId: variant.resourceId,
  };
}

/** Build a versioned, JSON-serializable snapshot of the current sniffer state. */
export function buildResourceDebugLog(options: ResourceDebugLogOptions) {
  const manifests = options.manifests || [];
  const representedIds = new Set(
    options.candidates.flatMap((candidate) => candidate.rawResourceIds),
  );
  const rawNonMediaCount = options.resources.filter(
    (resource) =>
      resource.type !== "video" &&
      resource.type !== "stream" &&
      !representedIds.has(resource.id),
  ).length;
  const mediaResourceCount = options.resources.filter(
    (resource) =>
      resource.type === "video" ||
      resource.type === "audio" ||
      resource.type === "stream",
  ).length;

  return {
    format: "fluxdown-resource-debug",
    schemaVersion: 1,
    exportedAt: new Date().toISOString(),
    source: options.source,
    context: {
      tabId: options.tabId,
      pageUrl: redactUrl(options.pageUrl),
      pageTitle: options.pageTitle || "",
    },
    summary: {
      rawResourceCount: options.resources.length,
      rawMediaResourceCount: mediaResourceCount,
      manifestCount: manifests.length,
      candidateCount: options.candidates.length,
      downloadableCandidateCount: options.candidates.filter(
        (candidate) => candidate.downloadable,
      ).length,
      representedRawResourceCount: representedIds.size,
      displayedRowCount: countMediaCandidateRows(options.candidates) + rawNonMediaCount,
    },
    resources: options.resources.map((resource) => ({
      id: resource.id,
      url: redactUrl(resource.url),
      canonicalUrl: canonicalUrl(resource.url),
      finalUrl: redactUrl(resource.finalUrl),
      canonicalFinalUrl: canonicalUrl(resource.finalUrl),
      filename: resource.filename,
      type: resource.type,
      size: resource.size,
      mimeType: resource.mimeType,
      quality: resource.quality,
      qualities: resource.qualities?.map((quality) => ({
        ...quality,
        url: redactUrl(quality.url),
        canonicalUrl: canonicalUrl(quality.url),
      })),
      detectedBy: resource.detectedBy,
      detectedAt: resource.detectedAt,
      detectedAtIso: timestampIso(resource.detectedAt),
      tabId: resource.tabId,
      pageUrl: redactUrl(resource.pageUrl),
      confidence: resource.confidence,
      isAttachment: resource.isAttachment,
      auth: {
        hasCookies: Boolean(resource.cookies),
        headerNames: Object.keys(resource.headers || {}).sort(),
      },
    })),
    manifests: manifests.map((entry) => ({
      url: redactUrl(entry.url),
      canonicalUrl: canonicalUrl(entry.url),
      video: entry.manifest.video.map(debugTrack),
      audio: entry.manifest.audio.map(debugTrack),
    })),
    candidates: options.candidates.map((candidate) => ({
      id: candidate.id,
      title: candidate.title,
      type: candidate.type,
      source: candidate.source,
      pageUrl: redactUrl(candidate.pageUrl),
      downloadable: candidate.downloadable,
      fragmentCount: candidate.fragmentCount,
      rawResourceIds: candidate.rawResourceIds,
      variants: candidate.variants.map(debugVariant),
    })),
  };
}

export function stringifyResourceDebugLog(log: ReturnType<typeof buildResourceDebugLog>): string {
  return `${JSON.stringify(log, null, 2)}\n`;
}
