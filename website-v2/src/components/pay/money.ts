import type { Lang } from "@/i18n/config";

/** Intl locale per site language (currency / date formatting). */
export const INTL_LOCALE: Record<Lang, string> = { en: "en-US", zh: "zh-CN" };

/** Minor units (cents / fen) → localized currency string; whole amounts drop decimals. */
export function formatMinor(minor: number, currency: string, lang: Lang): string {
  try {
    return new Intl.NumberFormat(INTL_LOCALE[lang], {
      style: "currency",
      currency,
      minimumFractionDigits: minor % 100 === 0 ? 0 : 2,
    }).format(minor / 100);
  } catch {
    return `${(minor / 100).toFixed(2)} ${currency}`;
  }
}

/** ISO date/time → localized short date; empty string for unparsable input. */
export function formatDate(iso: string, lang: Lang): string {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? "" : d.toLocaleDateString(INTL_LOCALE[lang]);
}
