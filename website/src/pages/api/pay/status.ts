import type { APIRoute } from "astro";
import { isPaid, markPaid, payConfigError, queryOrder } from "@/lib/pay";

export const prerender = false;

const JSON_HEADERS = { "Content-Type": "application/json", "Cache-Control": "no-store" };

// 异步回调只会打到网关上配置的那一个 notify URL；当回调落在另一个实例（内存状态不共享）时，
// 本实例的 Map 永远不会命中。所以本地未命中时向网关做带签名的查询兜底，状态与金额只信任网关响应。
const GATEWAY_LOOKUP_MIN_INTERVAL = 5_000;
const LOOKUP_MAP_MAX = 500;
const lastLookup = new Map<string, number>();

async function confirmViaGateway(outTradeNo: string): Promise<boolean> {
  if (payConfigError()) return false;
  const now = Date.now();
  const last = lastLookup.get(outTradeNo) ?? 0;
  if (now - last < GATEWAY_LOOKUP_MIN_INTERVAL) return false;
  if (lastLookup.size >= LOOKUP_MAP_MAX) {
    for (const [k, t] of lastLookup) {
      if (now - t >= GATEWAY_LOOKUP_MIN_INTERVAL) lastLookup.delete(k);
    }
    if (lastLookup.size >= LOOKUP_MAP_MAX) return false;
  }
  lastLookup.set(outTradeNo, now);
  try {
    const order = await queryOrder(outTradeNo);
    if (order.status !== "paid") return false;
    const cents = Math.round(Number(order.amount));
    markPaid(outTradeNo, Number.isFinite(cents) && cents > 0 ? cents : 0);
    return true;
  } catch (err) {
    console.error("[pay/status] gateway query failed:", err);
    return false;
  }
}

/* Frontend polls this to learn when an order's async callback arrived. */
export const GET: APIRoute = async ({ url }) => {
  const outTradeNo = url.searchParams.get("outTradeNo");
  if (!outTradeNo || outTradeNo.length > 128) {
    return new Response(JSON.stringify({ error: "outTradeNo required" }), {
      status: 400,
      headers: JSON_HEADERS,
    });
  }
  const paid = isPaid(outTradeNo) || (await confirmViaGateway(outTradeNo));
  return new Response(JSON.stringify({ paid }), {
    status: 200,
    headers: JSON_HEADERS,
  });
};
