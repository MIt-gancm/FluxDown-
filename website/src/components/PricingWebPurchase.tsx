import { useCallback, useEffect, useRef, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { QRCodeSVG } from "qrcode.react";
import { useLocale } from "@/lib/i18n";
import type { Messages } from "@/lib/locales";

/** 网页购买流程（定价页卡片入口）：账号确认 → 下单 → 微信扫码 → 轮询到账。 */

interface PlanBrief {
  code: string;
  name: string;
  /** 卡片当前展示价（活动生效价，分）；确认页据此预估应付，最终以订单为准。 */
  priceMinor: number;
  currency: string;
}
interface Identity {
  nickname: string;
  emailMasked: string;
  originId: number | null;
  planCode: string;
  planName: string;
  purchaseCreditMinor: number;
}
interface WebOrder {
  orderNo: string;
  planName: string;
  status: string;
  amountMinor: number;
  creditMinor: number;
  currency: string;
  codeUrl: string | null;
}

type Step = "account" | "confirm" | "pay" | "success";

/** FluxCloud 错误码 → 文案 key（未知码回退网络错误文案）。 */
const ERROR_KEYS: Record<string, keyof Messages> = {
  not_found: "webbuy.err.notFound",
  validation_error: "webbuy.err.invalidAccount",
  rate_limited: "webbuy.err.rateLimited",
  payment_disabled: "webbuy.err.paymentDisabled",
  plan_not_purchasable: "webbuy.err.notPurchasable",
  plan_already_owned: "webbuy.err.alreadyOwned",
  not_an_upgrade: "webbuy.err.notAnUpgrade",
  gateway_error: "webbuy.err.gateway",
  upstream_unreachable: "webbuy.err.gateway",
};

function formatMinor(minor: number, currency: string, locale: string): string {
  try {
    return new Intl.NumberFormat(locale === "zh" ? "zh-CN" : locale, {
      style: "currency",
      currency,
      minimumFractionDigits: minor % 100 === 0 ? 0 : 2,
    }).format(minor / 100);
  } catch {
    return `${(minor / 100).toFixed(2)} ${currency}`;
  }
}

async function parseError(res: Response): Promise<string> {
  try {
    const body = (await res.json()) as { code?: string };
    return body.code ?? "network";
  } catch {
    return "network";
  }
}

export default function WebPurchase({ plan }: { plan: PlanBrief }) {
  const { t, locale } = useLocale();
  const [open, setOpen] = useState(false);
  const [step, setStep] = useState<Step>("account");
  const [account, setAccount] = useState("");
  const [identity, setIdentity] = useState<Identity | null>(null);
  const [order, setOrder] = useState<WebOrder | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const pollRef = useRef<number | null>(null);

  const errText = useCallback(
    (code: string) => {
      const key = ERROR_KEYS[code];
      return key ? t(key) : t("webbuy.err.network");
    },
    [t],
  );

  const reset = useCallback(() => {
    setStep("account");
    setIdentity(null);
    setOrder(null);
    setError(null);
    setBusy(false);
  }, []);

  const close = useCallback(() => {
    setOpen(false);
    reset();
    setAccount("");
  }, [reset]);

  async function doLookup() {
    const trimmed = account.trim();
    if (!trimmed) return;
    setBusy(true);
    setError(null);
    try {
      const res = await fetch("/api/cloud/order-lookup", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ account: trimmed }),
      });
      if (!res.ok) {
        setError(errText(await parseError(res)));
        return;
      }
      setIdentity((await res.json()) as Identity);
      setStep("confirm");
    } catch {
      setError(t("webbuy.err.network"));
    } finally {
      setBusy(false);
    }
  }

  async function doCreateOrder() {
    setBusy(true);
    setError(null);
    try {
      const res = await fetch("/api/cloud/order", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ account: account.trim(), planCode: plan.code }),
      });
      if (!res.ok) {
        setError(errText(await parseError(res)));
        return;
      }
      setOrder((await res.json()) as WebOrder);
      setStep("pay");
    } catch {
      setError(t("webbuy.err.network"));
    } finally {
      setBusy(false);
    }
  }

  // 支付轮询：3s 一次查订单状态；paid → 成功页，expired/failed → 报错回下单。
  useEffect(() => {
    if (step !== "pay" || !order) return;
    const tick = async () => {
      try {
        const res = await fetch(
          `/api/cloud/order?orderNo=${encodeURIComponent(order.orderNo)}&account=${encodeURIComponent(account.trim())}`,
        );
        if (!res.ok) return; // 瞬时失败/限频：静默等下一轮
        const fresh = (await res.json()) as WebOrder;
        if (fresh.status === "paid") {
          setOrder(fresh);
          setStep("success");
        } else if (fresh.status === "expired" || fresh.status === "failed") {
          setError(
            t(
              fresh.status === "expired"
                ? "webbuy.err.expired"
                : "webbuy.err.failed",
            ),
          );
          setOrder(null);
          setStep("confirm");
        }
      } catch {
        // 网络抖动：忽略，等下一轮
      }
    };
    pollRef.current = window.setInterval(() => void tick(), 3000);
    return () => {
      if (pollRef.current != null) window.clearInterval(pollRef.current);
      pollRef.current = null;
    };
  }, [step, order, account, t]);

  const fieldRow = (label: string, value: string) => (
    <div className="flex items-center justify-between gap-4 py-2">
      <span className="text-xs text-dark-text-muted">{label}</span>
      <span className="text-sm font-medium text-dark-text">{value}</span>
    </div>
  );

  return (
    <>
      <button
        type="button"
        onClick={() => setOpen(true)}
        className="w-full inline-flex items-center justify-center gap-1.5 rounded-lg bg-brand-sky/90 hover:bg-brand-sky px-4 py-2.5 text-sm font-semibold text-dark-bg transition-colors"
      >
        {t("webbuy.button")}
      </button>

      <AnimatePresence>
        {open && (
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            className="fixed inset-0 z-[80] flex items-center justify-center bg-black/60 backdrop-blur-sm p-4"
            onClick={(e) => {
              if (e.target === e.currentTarget && step !== "pay") close();
            }}
          >
            <motion.div
              initial={{ opacity: 0, scale: 0.96, y: 12 }}
              animate={{ opacity: 1, scale: 1, y: 0 }}
              exit={{ opacity: 0, scale: 0.96, y: 12 }}
              transition={{ duration: 0.18 }}
              className="w-full max-w-md rounded-2xl border border-dark-border bg-dark-surface1 shadow-2xl overflow-hidden"
            >
              {/* Header */}
              <div className="flex items-center justify-between px-6 py-4 border-b border-dark-border">
                <h3 className="text-sm font-semibold text-dark-text">
                  {t("webbuy.title", { plan: plan.name })}
                </h3>
                <button
                  type="button"
                  onClick={close}
                  aria-label={t("webbuy.close")}
                  className="text-dark-text-muted hover:text-dark-text transition-colors"
                >
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                    <path d="M18 6 6 18" />
                    <path d="m6 6 12 12" />
                  </svg>
                </button>
              </div>

              <div className="px-6 py-5">
                {step === "account" && (
                  <div>
                    <label className="block text-xs font-medium text-dark-text-secondary" htmlFor="webbuy-account">
                      {t("webbuy.accountLabel")}
                    </label>
                    <input
                      id="webbuy-account"
                      value={account}
                      onChange={(e) => setAccount(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter" && !busy) void doLookup();
                      }}
                      placeholder={t("webbuy.accountPlaceholder")}
                      autoFocus
                      className="mt-2 w-full rounded-lg border border-dark-border bg-dark-bg px-3 py-2.5 text-sm text-dark-text placeholder:text-dark-text-muted focus:outline-none focus:border-brand-sky/60"
                    />
                    <p className="mt-2.5 text-xs text-dark-text-muted leading-relaxed">
                      {t("webbuy.accountHelp")}
                    </p>
                    {error && <p className="mt-3 text-xs text-danger">{error}</p>}
                    <button
                      type="button"
                      disabled={busy || !account.trim()}
                      onClick={() => void doLookup()}
                      className="mt-5 w-full rounded-lg bg-brand-sky/90 hover:bg-brand-sky disabled:opacity-50 disabled:cursor-not-allowed px-4 py-2.5 text-sm font-semibold text-dark-bg transition-colors"
                    >
                      {busy ? t("webbuy.loading") : t("webbuy.next")}
                    </button>
                  </div>
                )}

                {step === "confirm" && identity && (
                  <div>
                    <p className="text-xs text-dark-text-secondary leading-relaxed">
                      {t("webbuy.confirmDesc")}
                    </p>
                    <div className="mt-4 rounded-xl border border-dark-border bg-dark-bg/60 px-4 py-2 divide-y divide-dark-border/60">
                      {fieldRow(t("webbuy.fieldNickname"), identity.nickname)}
                      {fieldRow(t("webbuy.fieldEmail"), identity.emailMasked)}
                      {identity.originId != null &&
                        fieldRow(t("webbuy.fieldOrigin"), String(identity.originId))}
                      {fieldRow(t("webbuy.fieldPlan"), identity.planName)}
                    </div>
                    {/* 价格预估：套餐价 − 当前套餐抵扣 = 应付；最终金额以订单（cloud 侧）为准 */}
                    <div className="mt-3 rounded-xl border border-dark-border bg-dark-bg/60 px-4 py-2 divide-y divide-dark-border/60">
                      {fieldRow(
                        t("webbuy.pricePlan"),
                        formatMinor(plan.priceMinor, plan.currency, locale),
                      )}
                      {identity.purchaseCreditMinor > 0 &&
                        fieldRow(
                          t("webbuy.priceCredit"),
                          `− ${formatMinor(
                            Math.min(identity.purchaseCreditMinor, plan.priceMinor),
                            plan.currency,
                            locale,
                          )}`,
                        )}
                      {fieldRow(
                        t("webbuy.pricePayable"),
                        formatMinor(
                          Math.max(0, plan.priceMinor - identity.purchaseCreditMinor),
                          plan.currency,
                          locale,
                        ),
                      )}
                    </div>
                    {error && <p className="mt-3 text-xs text-danger">{error}</p>}
                    <div className="mt-5 flex gap-3">
                      <button
                        type="button"
                        onClick={() => {
                          reset();
                        }}
                        className="flex-1 rounded-lg border border-dark-border px-4 py-2.5 text-sm font-medium text-dark-text-secondary hover:text-dark-text hover:border-dark-text-muted transition-colors"
                      >
                        {t("webbuy.notMe")}
                      </button>
                      <button
                        type="button"
                        disabled={busy}
                        onClick={() => void doCreateOrder()}
                        className="flex-1 rounded-lg bg-brand-sky/90 hover:bg-brand-sky disabled:opacity-50 px-4 py-2.5 text-sm font-semibold text-dark-bg transition-colors"
                      >
                        {busy ? t("webbuy.loading") : t("webbuy.confirmPay")}
                      </button>
                    </div>
                  </div>
                )}

                {step === "pay" && order && order.codeUrl && (
                  <div className="text-center">
                    <p className="text-xs text-dark-text-secondary">{t("webbuy.scanTitle")}</p>
                    <div className="mt-4 inline-block rounded-xl bg-white p-4">
                      <QRCodeSVG value={order.codeUrl} size={192} />
                    </div>
                    <div className="mt-4 text-2xl font-bold text-dark-text tabular-nums">
                      {formatMinor(order.amountMinor, order.currency, locale)}
                    </div>
                    {order.creditMinor > 0 && (
                      <p className="mt-1 text-xs text-brand-sky">
                        {t("webbuy.credit", {
                          amount: formatMinor(order.creditMinor, order.currency, locale),
                        })}
                      </p>
                    )}
                    <p className="mt-3 inline-flex items-center gap-2 text-xs text-dark-text-muted">
                      <span className="inline-block h-3 w-3 rounded-full border-2 border-brand-sky/40 border-t-brand-sky animate-spin" />
                      {t("webbuy.waiting")}
                    </p>
                    <p className="mt-2 text-[11px] text-dark-text-muted/70 tabular-nums">
                      {t("webbuy.orderNo")}: {order.orderNo}
                    </p>
                  </div>
                )}

                {step === "success" && order && (
                  <div className="text-center py-4">
                    <div className="mx-auto flex h-12 w-12 items-center justify-center rounded-full bg-brand-sky/15 text-brand-sky">
                      <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                        <polyline points="20 6 9 17 4 12" />
                      </svg>
                    </div>
                    <h4 className="mt-4 text-base font-semibold text-dark-text">
                      {t("webbuy.paidTitle")}
                    </h4>
                    <p className="mt-2 text-xs text-dark-text-secondary leading-relaxed">
                      {t("webbuy.paidDesc")}
                    </p>
                    <button
                      type="button"
                      onClick={close}
                      className="mt-5 w-full rounded-lg bg-brand-sky/90 hover:bg-brand-sky px-4 py-2.5 text-sm font-semibold text-dark-bg transition-colors"
                    >
                      {t("webbuy.close")}
                    </button>
                  </div>
                )}
              </div>
            </motion.div>
          </motion.div>
        )}
      </AnimatePresence>
    </>
  );
}
