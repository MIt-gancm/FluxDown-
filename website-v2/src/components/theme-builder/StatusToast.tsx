/** 构建器底部的操作结果提示（导入 / 导出 / 复制 / 重置），Flutter 与 GPUI 编辑器共用。 */
import { X } from "lucide-react";
import { cn } from "@/lib/utils";

export type Status = { type: "success" | "error"; text: string };

export function StatusToast({ status, onDismiss, dismissLabel }: { status: Status | null; onDismiss: () => void; dismissLabel: string }) {
  return (
    <div aria-live="polite" className="pointer-events-none absolute inset-x-0 bottom-4 flex justify-center px-4">
      {status && (
        <div
          role={status.type === "error" ? "alert" : "status"}
          className={cn(
            "pointer-events-auto flex items-center gap-3 rounded-md border bg-elev py-1.5 pl-3 pr-1.5 text-[13px] shadow-[var(--shadow-float)] transition-[opacity,translate] duration-200 starting:translate-y-2 starting:opacity-0",
            status.type === "success" ? "border-[color-mix(in_oklch,var(--ok)_45%,transparent)] text-ok" : "border-[color-mix(in_oklch,var(--danger)_45%,transparent)] text-danger",
          )}
        >
          <span>{status.text}</span>
          <button
            type="button"
            onClick={onDismiss}
            aria-label={dismissLabel}
            className="grid h-6 w-6 place-items-center rounded text-muted hover:bg-inset hover:text-fg"
          >
            <X size={13} />
          </button>
        </div>
      )}
    </div>
  );
}
