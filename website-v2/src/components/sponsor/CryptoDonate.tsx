import { useRef, useState } from "react";
import { ArrowUpRight, Check, Copy, TriangleAlert } from "lucide-react";
import type { Lang } from "@/i18n/config";
import { sponsor } from "@/i18n/messages/sponsor";
import Dialog from "@/components/pay/Dialog";
import { QrTile } from "@/components/pay/WechatQr";
import { NETWORKS, NetworkLogo, type Network } from "./networks";

/** Crypto donations: pick a network → dialog with address QR, copyable address and a network warning. */
export default function CryptoDonate({ lang }: { lang: Lang }) {
  const t = sponsor[lang].crypto;
  const [active, setActive] = useState<Network | null>(null);
  const [copied, setCopied] = useState(false);
  const addressRef = useRef<HTMLElement>(null);

  async function copy(address: string) {
    try {
      await navigator.clipboard.writeText(address);
    } catch {
      // Clipboard API unavailable (insecure context / denied): select the address for a manual copy.
      if (addressRef.current) window.getSelection()?.selectAllChildren(addressRef.current);
      return;
    }
    setCopied(true);
    window.setTimeout(() => setCopied(false), 2000);
  }

  return (
    <div>
      <p className="mb-4 text-[13px] text-muted">{t.hint}</p>
      <ul className="cells border border-line sm:grid-cols-2">
        {NETWORKS.map((n) => (
          <li key={n.id}>
            <button
              type="button"
              onClick={() => {
                setCopied(false);
                setActive(n);
              }}
              className="spot group flex w-full items-center gap-3.5 p-4 text-left transition-colors hover:bg-sunken focus-visible:bg-sunken"
            >
              <span className="grid size-10 shrink-0 place-items-center rounded-[8px] bg-elev shadow-[0_0_0_1px_var(--line)_inset]">
                <NetworkLogo id={n.id} />
              </span>
              <span className="min-w-0 flex-1">
                <span className="block text-sm font-medium">{t.networks[n.id]}</span>
                <span className="mt-0.5 block font-mono text-[11px] text-subtle">{n.assets}</span>
              </span>
              <ArrowUpRight
                size={15}
                className="shrink-0 text-subtle transition-transform group-hover:-translate-y-0.5 group-hover:translate-x-0.5 group-hover:text-accent"
                aria-hidden
              />
              <span className="sr-only">{t.open}</span>
            </button>
          </li>
        ))}
      </ul>

      <Dialog open={active != null} onClose={() => setActive(null)} title={active ? t.networks[active.id] : ""} closeLabel={t.close}>
        {active && (
          <div className="flex flex-col items-center text-center">
            <span className="grid size-12 place-items-center rounded-[10px] bg-sunken shadow-[0_0_0_1px_var(--line)_inset]">
              <NetworkLogo id={active.id} size={26} />
            </span>
            <p className="mt-2 font-mono text-xs text-muted">{active.assets}</p>
            <div className="mt-5">
              <QrTile value={active.address} size={196} title={t.address} />
            </div>
            <p className="eyebrow-plain mt-6 self-start">{t.address}</p>
            <code ref={addressRef} className="mt-2 block w-full select-all break-all rounded-[8px] bg-sunken px-3 py-2.5 text-left font-mono text-[13px] shadow-[0_0_0_1px_var(--line)_inset]">
              {active.address}
            </code>
            <button type="button" onClick={() => void copy(active.address)} className="btn btn-secondary btn-sm mt-3 self-start">
              {copied ? <Check size={14} className="text-ok" aria-hidden /> : <Copy size={14} aria-hidden />}
              <span aria-live="polite">{copied ? t.copied : t.copy}</span>
            </button>
            <p className="mt-5 flex gap-2 border-l-2 border-danger bg-sunken px-3 py-2.5 text-left text-xs leading-relaxed text-danger">
              <TriangleAlert size={14} className="mt-px shrink-0" aria-hidden />
              {t.notice(t.networks[active.id])}
            </p>
          </div>
        )}
      </Dialog>
    </div>
  );
}
