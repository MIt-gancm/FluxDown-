import { useCallback, useEffect, useRef, useState } from "react";
import { CircleCheck, Heart, LoaderCircle, RefreshCw } from "lucide-react";
import type { Lang } from "@/i18n/config";
import { sponsor } from "@/i18n/messages/sponsor";
import Dialog from "@/components/pay/Dialog";
import { QrTile, WechatBadge } from "@/components/pay/WechatQr";
import { SPONSOR_WALL_URL } from "./wall";

/**
 * Pay-what-you-want sponsorship (zerx pay gateway):
 * pick a preset or custom amount → POST /api/pay/create → WeChat QR → poll /api/pay/status
 * until paid → fire-and-forget POST /api/sponsor/wall with the optional name/message.
 */

const PRESET_AMOUNTS = [5, 15, 30, 66, 128];
const POLL_INTERVAL = 2500;
const POLL_TIMEOUT = 5 * 60 * 1000;

type PayState =
  | { phase: "idle" }
  | { phase: "creating" }
  | { phase: "pending"; codeUrl: string; outTradeNo: string }
  | { phase: "paid" }
  | { phase: "error"; message: string };

export default function WechatSponsor({ lang }: { lang: Lang }) {
  const t = sponsor[lang];
  const [selected, setSelected] = useState(PRESET_AMOUNTS[1]!);
  const [custom, setCustom] = useState("");
  const [name, setName] = useState("");
  const [message, setMessage] = useState("");
  const [wallQueued, setWallQueued] = useState(false);
  const [pay, setPay] = useState<PayState>({ phase: "idle" });

  // Latest name/message for the long-lived poll closure without re-arming timers.
  const wallInfo = useRef({ name: "", message: "" });
  useEffect(() => {
    wallInfo.current = { name, message };
  }, [name, message]);

  const pollTimer = useRef<number | null>(null);
  const pollDeadline = useRef(0);

  // Custom amount overrides the preset when it is a positive number.
  const customNum = parseFloat(custom);
  const amountYuan = custom.trim() !== "" && Number.isFinite(customNum) && customNum > 0 ? customNum : selected;
  const amountValid = Number.isFinite(amountYuan) && amountYuan >= 1;
  const customTooSmall = custom.trim() !== "" && Number.isFinite(customNum) && customNum > 0 && customNum < 1;

  const stopPolling = useCallback(() => {
    if (pollTimer.current != null) window.clearTimeout(pollTimer.current);
    pollTimer.current = null;
  }, []);

  useEffect(() => stopPolling, [stopPolling]);

  const poll = useCallback(
    (outTradeNo: string) => {
      const tick = async () => {
        if (Date.now() > pollDeadline.current) {
          stopPolling();
          setPay({ phase: "error", message: t.pay.timeout });
          return;
        }
        try {
          const res = await fetch(`/api/pay/status?outTradeNo=${encodeURIComponent(outTradeNo)}`);
          if (res.ok) {
            const data = (await res.json()) as { paid?: boolean };
            if (data.paid) {
              stopPolling();
              // Anonymous sponsors are recorded too (server falls back to a default name).
              // Fire-and-forget: the thank-you screen never waits on GitHub.
              setWallQueued(true);
              void fetch("/api/sponsor/wall", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({
                  outTradeNo,
                  name: wallInfo.current.name.trim(),
                  message: wallInfo.current.message.trim(),
                }),
              }).catch(() => {});
              setPay({ phase: "paid" });
              return;
            }
          }
        } catch {
          // transient: keep polling
        }
        pollTimer.current = window.setTimeout(tick, POLL_INTERVAL);
      };
      pollTimer.current = window.setTimeout(tick, POLL_INTERVAL);
    },
    [stopPolling, t],
  );

  async function startPayment() {
    if (!amountValid) return;
    stopPolling();
    setWallQueued(false);
    setPay({ phase: "creating" });
    try {
      const res = await fetch("/api/pay/create", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ amountCents: Math.round(amountYuan * 100), subject: "Support FluxDown" }),
      });
      if (!res.ok) {
        const data = (await res.json().catch(() => ({}))) as { error?: string };
        setPay({
          phase: "error",
          message: res.status === 503 ? t.pay.unavailable : data.error || t.pay.failed,
        });
        return;
      }
      const data = (await res.json()) as { codeUrl?: string; outTradeNo?: string };
      if (!data.codeUrl || !data.outTradeNo) {
        setPay({ phase: "error", message: t.pay.failed });
        return;
      }
      pollDeadline.current = Date.now() + POLL_TIMEOUT;
      setPay({ phase: "pending", codeUrl: data.codeUrl, outTradeNo: data.outTradeNo });
      poll(data.outTradeNo);
    } catch {
      setPay({ phase: "error", message: t.pay.failed });
    }
  }

  const closeDialog = () => {
    stopPolling();
    setPay({ phase: "idle" });
  };

  return (
    <>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void startPayment();
        }}
        className="flex flex-col gap-6"
      >
        <fieldset>
          <legend className="eyebrow-plain mb-3">{t.amountLabel}</legend>
          <div className="grid grid-cols-3 gap-2 sm:grid-cols-5">
            {PRESET_AMOUNTS.map((amt) => {
              const active = custom.trim() === "" && selected === amt;
              return (
                <button
                  key={amt}
                  type="button"
                  aria-pressed={active}
                  onClick={() => {
                    setSelected(amt);
                    setCustom("");
                  }}
                  className={`num h-11 rounded-[8px] font-mono text-[15px] font-medium transition-[box-shadow,background-color,color] duration-150 ${
                    active
                      ? "bg-accent-soft text-accent-ink shadow-[0_0_0_1px_var(--accent)_inset]"
                      : "bg-elev text-muted shadow-[0_0_0_1px_var(--line-strong)_inset] hover:text-fg hover:shadow-[0_0_0_1px_var(--fg-subtle)_inset]"
                  }`}
                >
                  ¥{amt}
                </button>
              );
            })}
          </div>
          <label htmlFor="sponsor-custom" className="sr-only">
            {t.customLabel}
          </label>
          <div className="relative mt-2">
            <span aria-hidden className="pointer-events-none absolute left-3.5 top-1/2 -translate-y-1/2 font-mono text-sm text-subtle">
              ¥
            </span>
            <input
              id="sponsor-custom"
              type="number"
              min={1}
              step="any"
              inputMode="decimal"
              value={custom}
              onChange={(e) => setCustom(e.target.value)}
              placeholder={t.customPlaceholder}
              aria-invalid={customTooSmall || undefined}
              aria-describedby={customTooSmall ? "sponsor-custom-err" : undefined}
              className="field num h-11 pl-8 font-mono"
            />
          </div>
          {customTooSmall && (
            <p id="sponsor-custom-err" className="mt-2 text-xs text-danger">
              {t.minAmount}
            </p>
          )}
        </fieldset>

        <div className="grid gap-3">
          <div>
            <label htmlFor="sponsor-name" className="mb-1.5 block text-[13px] font-medium">
              {t.nameLabel}
            </label>
            <input
              id="sponsor-name"
              type="text"
              maxLength={30}
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder={t.namePlaceholder}
              autoComplete="nickname"
              className="field"
            />
          </div>
          <div>
            <label htmlFor="sponsor-message" className="mb-1.5 block text-[13px] font-medium">
              {t.messageLabel}
            </label>
            <textarea
              id="sponsor-message"
              rows={2}
              maxLength={300}
              value={message}
              onChange={(e) => setMessage(e.target.value)}
              placeholder={t.messagePlaceholder}
              className="field resize-none"
            />
          </div>
          <p className="text-xs leading-relaxed text-muted">
            {t.wallHint}{" "}
            <a href={SPONSOR_WALL_URL} target="_blank" rel="noopener noreferrer" className="link">
              {t.wallLink}
            </a>
          </p>
        </div>

        <div>
          <button
            type="submit"
            disabled={!amountValid || pay.phase === "creating"}
            className="btn btn-primary btn-lg w-full disabled:cursor-not-allowed disabled:opacity-50"
          >
            {pay.phase === "creating" ? (
              <LoaderCircle size={16} className="animate-spin" aria-hidden />
            ) : (
              <Heart size={16} aria-hidden />
            )}
            {t.cta}
            {amountValid && <span className="num font-mono opacity-80">· ¥{amountYuan}</span>}
          </button>
          <p className="mt-3 text-center text-xs text-subtle">{t.ctaHint}</p>
        </div>
      </form>

      <Dialog
        open={pay.phase === "pending" || pay.phase === "paid" || pay.phase === "error"}
        onClose={closeDialog}
        title={t.pay.dialogTitle}
        closeLabel={t.pay.close}
      >
        {pay.phase === "pending" && (
          <div className="flex flex-col items-center text-center">
            <WechatBadge label={t.methods.wechat} />
            <h3 className="mt-3 text-base font-semibold">{t.pay.scanTitle}</h3>
            <p className="mt-1 text-xs text-muted">{t.pay.scanHint}</p>
            <div className="mt-5">
              <QrTile value={pay.codeUrl} size={196} wechat />
            </div>
            <p className="num mt-5 font-mono text-2xl font-semibold">¥{amountYuan}</p>
            <p role="status" className="mt-3 inline-flex items-center gap-2 text-xs text-muted">
              <LoaderCircle size={13} className="animate-spin text-accent" aria-hidden />
              {t.pay.waiting}
            </p>
          </div>
        )}
        {pay.phase === "paid" && (
          <div role="status" className="flex flex-col items-center py-2 text-center">
            <span className="grid size-14 place-items-center rounded-full bg-[color-mix(in_oklch,var(--ok)_14%,transparent)] text-ok">
              <CircleCheck size={28} aria-hidden />
            </span>
            <h3 className="mt-4 text-lg font-semibold">{t.pay.thanksTitle}</h3>
            <p className="mt-1 text-sm text-muted">{t.pay.thanksBody}</p>
            {wallQueued && <p className="mt-2 text-xs text-subtle">{t.pay.thanksNote}</p>}
            <a href={SPONSOR_WALL_URL} target="_blank" rel="noopener noreferrer" className="link mt-5 text-xs">
              {t.wallLink} →
            </a>
          </div>
        )}
        {pay.phase === "error" && (
          <div role="alert" className="flex flex-col items-center py-2 text-center">
            <h3 className="text-base font-semibold">{t.pay.errorTitle}</h3>
            <p className="mt-2 text-sm text-muted">{pay.message}</p>
            <button type="button" onClick={() => void startPayment()} className="btn btn-secondary mt-6">
              <RefreshCw size={14} aria-hidden />
              {t.pay.retry}
            </button>
          </div>
        )}
      </Dialog>
    </>
  );
}
