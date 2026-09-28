/** 复制到剪贴板按钮(Clipboard API;不可用时按钮不给出成功反馈,号码本身可手动选中复制)。 */
import { useEffect, useState } from "react";
import { Check, Copy } from "lucide-react";

export default function CopyButton({
  value,
  label,
  copiedLabel,
  ariaLabel,
}: {
  value: string;
  label: string;
  copiedLabel: string;
  ariaLabel: string;
}) {
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!copied) return;
    const id = window.setTimeout(() => setCopied(false), 2000);
    return () => window.clearTimeout(id);
  }, [copied]);

  const copy = () =>
    navigator.clipboard?.writeText(value).then(
      () => setCopied(true),
      () => {},
    );

  return (
    <button type="button" className="btn btn-secondary btn-sm" aria-label={ariaLabel} onClick={copy}>
      {copied ? <Check aria-hidden className="size-3.5 text-ok" /> : <Copy aria-hidden className="size-3.5" />}
      <span aria-live="polite">{copied ? copiedLabel : label}</span>
    </button>
  );
}
