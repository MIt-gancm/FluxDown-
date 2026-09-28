import { useRef, useState, type KeyboardEvent } from "react";
import { Coins, CreditCard } from "lucide-react";
import type { Lang } from "@/i18n/config";
import { sponsor } from "@/i18n/messages/sponsor";
import WechatSponsor from "./WechatSponsor";
import CryptoDonate from "./CryptoDonate";

type Method = "wechat" | "crypto";
const METHODS: { id: Method; Icon: typeof CreditCard }[] = [
  { id: "wechat", Icon: CreditCard },
  { id: "crypto", Icon: Coins },
];

/** Sponsorship card: WAI-ARIA tabs switching WeChat pay-what-you-want and crypto wallets. */
export default function SponsorSupport({ lang }: { lang: Lang }) {
  const t = sponsor[lang];
  const [method, setMethod] = useState<Method>("wechat");
  const tabs = useRef<Record<Method, HTMLButtonElement | null>>({ wechat: null, crypto: null });

  function onKeyDown(e: KeyboardEvent) {
    if (e.key !== "ArrowLeft" && e.key !== "ArrowRight" && e.key !== "Home" && e.key !== "End") return;
    e.preventDefault();
    const idx = METHODS.findIndex((m) => m.id === method);
    const next =
      e.key === "Home" ? 0 : e.key === "End" ? METHODS.length - 1 : (idx + (e.key === "ArrowRight" ? 1 : -1) + METHODS.length) % METHODS.length;
    const id = METHODS[next]!.id;
    setMethod(id);
    tabs.current[id]?.focus();
  }

  return (
    <div className="fig">
      <div className="fig-bar">
        <span>fig. 01</span>
        <span>{t.figCaption[method]}</span>
      </div>
      <div className="p-4 sm:p-6">
        <div
          role="tablist"
          aria-label={t.methodsLabel}
          onKeyDown={onKeyDown}
          className="mb-6 grid grid-cols-2 gap-1 rounded-[10px] bg-sunken p-1 shadow-[0_0_0_1px_var(--line)_inset]"
        >
          {METHODS.map(({ id, Icon }) => {
            const selected = method === id;
            return (
              <button
                key={id}
                ref={(el) => {
                  tabs.current[id] = el;
                }}
                id={`sponsor-tab-${id}`}
                type="button"
                role="tab"
                aria-selected={selected}
                aria-controls={`sponsor-panel-${id}`}
                tabIndex={selected ? 0 : -1}
                onClick={() => setMethod(id)}
                className={`inline-flex h-9 items-center justify-center gap-2 rounded-[7px] text-[13px] font-medium transition-[background-color,color,box-shadow] duration-150 ${
                  selected ? "bg-elev text-fg shadow-[0_0_0_1px_var(--line-strong),0_1px_2px_oklch(0_0_0/0.06)]" : "text-muted hover:text-fg"
                }`}
              >
                <Icon size={15} aria-hidden />
                {t.methods[id]}
              </button>
            );
          })}
        </div>
        {METHODS.map(({ id }) => (
          <div key={id} id={`sponsor-panel-${id}`} role="tabpanel" aria-labelledby={`sponsor-tab-${id}`} hidden={method !== id}>
            {id === "wechat" ? <WechatSponsor lang={lang} /> : <CryptoDonate lang={lang} />}
          </div>
        ))}
      </div>
    </div>
  );
}
