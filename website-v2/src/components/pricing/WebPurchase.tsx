import { useCallback, useEffect, useState } from "react";
import { CircleCheck, LoaderCircle } from "lucide-react";
import type { Lang } from "@/i18n/config";
import { pricing } from "@/i18n/messages/pricing";
import Dialog from "@/components/pay/Dialog";
import { QrTile, WechatBadge } from "@/components/pay/WechatQr";
import { formatMinor } from "@/components/pay/money";
import type { PlanBrief } from "./types";
import { withBase } from "@/lib/base";

/** Web purchase from a plan card: account lookup → confirm → WeChat QR → poll until paid. */

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
type Errors = (typeof pricing)["en"]["webbuy"]["err"];

/** FluxCloud error code → message key (unknown codes fall back to the network message). */
const ERROR_KEYS: Record<string, keyof Errors> = {
  not_found: "notFound",
  validation_error: "invalidAccount",
  rate_limited: "rateLimited",
  payment_disabled: "paymentDisabled",
  plan_not_purchasable: "notPurchasable",
  plan_already_owned: "alreadyOwned",
  not_an_upgrade: "notAnUpgrade",
  plan_tier_not_higher: "tierNotHigher",
  gateway_error: "gateway",
  upstream_unreachable: "gateway",
};

const POLL_INTERVAL = 3000;

async function errorCode(res: Response): Promise<string> {
  try {
    const body = (await res.json()) as { code?: string };
    return body.code ?? "network";
  } catch {
    return "network";
  }
}

const STEP_INDEX: Record<Step, number> = { account: 0, confirm: 1, pay: 2, success: 3 };

export default function WebPurchase({ plan, lang }: { plan: PlanBrief; lang: Lang }) {
  const t = pricing[lang].webbuy;
  const [open, setOpen] = useState(false);
  const [step, setStep] = useState<Step>("account");
  const [account, setAccount] = useState("");
  const [identity, setIdentity] = useState<Identity | null>(null);
  const [order, setOrder] = useState<WebOrder | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

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

  async function lookup() {
    const trimmed = account.trim();
    if (!trimmed || busy) return;
    setBusy(true);
    setError(null);
    try {
      const res = await fetch(withBase("/api/cloud/order-lookup"), {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ account: trimmed }),
      });
      if (!res.ok) {
        setError(t.err[ERROR_KEYS[await errorCode(res)] ?? "network"]);
        return;
      }
      setIdentity((await res.json()) as Identity);
      setStep("confirm");
    } catch {
      setError(t.err.network);
    } finally {
      setBusy(false);
    }
  }

  async function createOrder() {
    setBusy(true);
    setError(null);
    try {
      const res = await fetch(withBase("/api/cloud/order"), {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ account: account.trim(), planCode: plan.code }),
      });
      if (!res.ok) {
        setError(t.err[ERROR_KEYS[await errorCode(res)] ?? "network"]);
        return;
      }
      setOrder((await res.json()) as WebOrder);
      setStep("pay");
    } catch {
      setError(t.err.network);
    } finally {
      setBusy(false);
    }
  }

  // Poll the order every 3 s: paid → success; expired/failed → back to confirm with an error.
  useEffect(() => {
    if (step !== "pay" || !order) return;
    const query = withBase(`/api/cloud/order?orderNo=${encodeURIComponent(order.orderNo)}&account=${encodeURIComponent(account.trim())}`);
    const timer = window.setInterval(async () => {
      try {
        const res = await fetch(query);
        if (!res.ok) return; // transient failure / rate limit: wait for the next tick
        const fresh = (await res.json()) as WebOrder;
        if (fresh.status === "paid") {
          setOrder(fresh);
          setStep("success");
        } else if (fresh.status === "expired" || fresh.status === "failed") {
          setError(fresh.status === "expired" ? t.err.expired : t.err.failed);
          setOrder(null);
          setStep("confirm");
        }
      } catch {
        // network blip: next tick
      }
    }, POLL_INTERVAL);
    return () => window.clearInterval(timer);
  }, [step, order, account, t]);

  const money = (minor: number, currency = plan.currency) => formatMinor(minor, currency, lang);

  return (
    <>
      <button type="button" onClick={() => setOpen(true)} className="btn btn-primary w-full">
        {t.button}
      </button>

      <Dialog open={open} onClose={close} title={t.title(plan.name)} closeLabel={t.close} dismissible={step !== "pay"}>
        {step !== "success" && (
          <ol className="mb-6 flex items-center gap-2 font-mono text-[10.5px] uppercase tracking-[0.1em]">
            {t.steps.map((label, i) => {
              const current = STEP_INDEX[step];
              return (
                <li
                  key={label}
                  aria-current={i === current ? "step" : undefined}
                  className={`flex items-center gap-2 last:flex-none ${i < t.steps.length - 1 ? "flex-1" : ""} ${
                    i > current ? "text-subtle" : i === current ? "text-accent-ink" : "text-muted"
                  }`}
                >
                  <span className="num">{String(i + 1).padStart(2, "0")}</span>
                  <span className="truncate">{label}</span>
                  {i < t.steps.length - 1 && (
                    <span aria-hidden className={`h-px min-w-3 flex-1 ${i < current ? "bg-accent" : "bg-line-strong"}`} />
                  )}
                </li>
              );
            })}
          </ol>
        )}

        {step === "account" && (
          <form
            onSubmit={(e) => {
              e.preventDefault();
              void lookup();
            }}
          >
            <label htmlFor={`webbuy-${plan.code}`} className="block text-[13px] font-medium">
              {t.accountLabel}
            </label>
            <input
              id={`webbuy-${plan.code}`}
              value={account}
              onChange={(e) => setAccount(e.target.value)}
              placeholder={t.accountPlaceholder}
              autoComplete="username"
              autoFocus
              aria-invalid={error ? true : undefined}
              aria-describedby={`webbuy-${plan.code}-help`}
              className="field mt-2"
            />
            <p id={`webbuy-${plan.code}-help`} className="mt-2.5 text-[12.5px] leading-relaxed text-muted">
              {t.accountHelp}
            </p>
            <ErrorLine text={error} />
            <button type="submit" disabled={busy || !account.trim()} className="btn btn-primary mt-5 w-full disabled:cursor-not-allowed disabled:opacity-50">
              {busy && <LoaderCircle size={15} className="animate-spin" aria-hidden />}
              {busy ? t.loading : t.next}
            </button>
          </form>
        )}

        {step === "confirm" && identity && (
          <div>
            <p className="text-[13px] leading-relaxed text-muted">{t.confirmDesc}</p>
            <dl className="mt-4 divide-y divide-line border-y border-line">
              <Row label={t.fieldNickname} value={identity.nickname} />
              <Row label={t.fieldEmail} value={identity.emailMasked} />
              {identity.originId != null && <Row label={t.fieldOrigin} value={String(identity.originId)} mono />}
              <Row label={t.fieldPlan} value={identity.planName} />
            </dl>
            {/* Estimate: plan price − current-plan credit = payable; the order amount is authoritative. */}
            <dl className="mt-4 divide-y divide-line border-y border-line">
              <Row label={t.pricePlan} value={money(plan.priceMinor)} mono />
              {identity.purchaseCreditMinor > 0 && (
                <Row label={t.priceCredit} value={`− ${money(Math.min(identity.purchaseCreditMinor, plan.priceMinor))}`} mono />
              )}
              <Row
                label={t.pricePayable}
                value={money(Math.max(0, plan.priceMinor - identity.purchaseCreditMinor))}
                mono
                strong
              />
            </dl>
            <ErrorLine text={error} />
            <div className="mt-6 grid grid-cols-2 gap-3">
              <button type="button" onClick={reset} className="btn btn-secondary">
                {t.notMe}
              </button>
              <button type="button" disabled={busy} onClick={() => void createOrder()} className="btn btn-primary disabled:opacity-50">
                {busy && <LoaderCircle size={15} className="animate-spin" aria-hidden />}
                {busy ? t.loading : t.confirmPay}
              </button>
            </div>
          </div>
        )}

        {step === "pay" && order && (
          <div className="flex flex-col items-center text-center">
            <WechatBadge label={t.wechatBadge} />
            <p className="mt-3 text-[13px] text-muted">{t.scanTitle}</p>
            {order.codeUrl ? (
              <div className="mt-5">
                <QrTile value={order.codeUrl} size={188} wechat />
              </div>
            ) : (
              <ErrorLine text={t.err.gateway} />
            )}
            <div className="num mt-5 text-3xl font-semibold tracking-tight">{money(order.amountMinor, order.currency)}</div>
            {order.creditMinor > 0 && (
              <p className="mt-1 text-xs text-accent-ink">{t.credit(money(order.creditMinor, order.currency))}</p>
            )}
            <p className="mt-4 inline-flex items-center gap-2 text-xs text-muted" role="status">
              <LoaderCircle size={13} className="animate-spin text-accent" aria-hidden />
              {t.waiting}
            </p>
            <p className="num mt-2 font-mono text-[11px] text-subtle">
              {t.orderNo} · {order.orderNo}
            </p>
          </div>
        )}

        {step === "success" && order && (
          <div className="flex flex-col items-center py-2 text-center" role="status">
            <span className="grid size-12 place-items-center rounded-full bg-accent-soft text-accent-ink">
              <CircleCheck size={24} aria-hidden />
            </span>
            <h3 className="mt-4 text-base font-semibold">{t.paidTitle}</h3>
            <p className="mt-2 text-[13px] leading-relaxed text-muted">{t.paidDesc}</p>
            <button type="button" onClick={close} className="btn btn-primary mt-6 w-full">
              {t.close}
            </button>
          </div>
        )}
      </Dialog>
    </>
  );
}

function Row({ label, value, mono, strong }: { label: string; value: string; mono?: boolean; strong?: boolean }) {
  return (
    <div className="flex items-center justify-between gap-4 py-2.5">
      <dt className="text-xs text-muted">{label}</dt>
      <dd className={`text-right text-sm ${mono ? "num font-mono" : ""} ${strong ? "font-semibold text-fg" : "font-medium"}`}>
        {value}
      </dd>
    </div>
  );
}

function ErrorLine({ text }: { text: string | null }) {
  if (!text) return null;
  return (
    <p role="alert" className="mt-3 text-[13px] text-danger">
      {text}
    </p>
  );
}
