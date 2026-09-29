/**
 * /docs/ 入口(SSR):按 cookie(用户主动选择过语言)或 Accept-Language
 * 302 到对应语言的文档首页。
 */
import type { APIRoute } from "astro";
import { parseAcceptLanguage } from "@/lib/locale-detect";
import { withBase } from "@/lib/base";

export const prerender = false;

export const GET: APIRoute = ({ cookies, request, redirect }) => {
  const cookie = cookies.get("fluxdown-locale")?.value;
  const lang =
    cookie === "zh" || cookie === "en" ? cookie : parseAcceptLanguage(request.headers.get("accept-language"));
  return redirect(withBase(`/docs/${lang}/`), 302);
};
