/**
 * 文档页反馈(client:visible):文字建议 → POST /api/feedback(type: "docs")。
 */
import { useId, useState, type SubmitEvent } from "react";
import { Check, MessageSquareText } from "lucide-react";
import type { Lang } from "@/i18n/config";
import { docs } from "@/i18n/messages/docs";

interface Props {
  pagePath: string;
  lang: Lang;
}

type Status = "idle" | "sending" | "done" | "error";

export default function DocsFeedbackForm({ pagePath, lang }: Props) {
  const t = docs[lang].feedback;
  const id = useId();
  const [text, setText] = useState("");
  const [contact, setContact] = useState("");
  const [status, setStatus] = useState<Status>("idle");

  async function submit(event: SubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    const description = text.trim();
    if (!description || status === "sending") return;
    setStatus("sending");
    try {
      const res = await fetch("/api/feedback", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          type: "docs",
          title: `[Docs] ${pagePath}`,
          description,
          contact: contact.trim() || undefined,
          pagePath,
        }),
      });
      if (!res.ok) throw new Error(String(res.status));
      setStatus("done");
      setText("");
      setContact("");
    } catch {
      setStatus("error");
    }
  }

  if (status === "done") {
    return (
      <p className="mt-10 flex items-center gap-2 rounded-[10px] border border-line bg-elev px-4 py-3 text-sm text-fg" role="status">
        <Check size={15} className="text-ok" aria-hidden="true" />
        {t.done}
      </p>
    );
  }

  return (
    <form onSubmit={submit} className="mt-10 rounded-[10px] border border-line bg-elev p-4 sm:p-5">
      <h2 className="flex items-center gap-2 text-sm font-medium text-fg">
        <MessageSquareText size={15} strokeWidth={1.75} className="text-subtle" aria-hidden="true" />
        {t.heading}
      </h2>
      <label htmlFor={`${id}-text`} className="sr-only">
        {t.label}
      </label>
      <textarea
        id={`${id}-text`}
        value={text}
        onChange={(e) => setText(e.target.value)}
        placeholder={t.placeholder}
        rows={3}
        maxLength={2000}
        required
        className="field mt-3 !h-auto resize-y py-2 text-sm leading-relaxed"
      />
      <div className="mt-2 flex flex-col gap-2 sm:flex-row">
        <label htmlFor={`${id}-contact`} className="sr-only">
          {t.contact}
        </label>
        <input
          id={`${id}-contact`}
          type="text"
          value={contact}
          onChange={(e) => setContact(e.target.value)}
          placeholder={t.contact}
          maxLength={200}
          autoComplete="email"
          className="field flex-1 text-sm"
        />
        <button
          type="submit"
          disabled={status === "sending" || text.trim().length === 0}
          className="btn btn-secondary disabled:cursor-not-allowed disabled:opacity-50"
        >
          {status === "sending" ? t.sending : t.submit}
        </button>
      </div>
      {status === "error" && (
        <p className="mt-2 text-xs text-danger" role="alert">
          {t.error}
        </p>
      )}
    </form>
  );
}
