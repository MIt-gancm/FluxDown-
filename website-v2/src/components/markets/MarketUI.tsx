/**
 * 插件 / 主题市场共用的界面零件:搜索框、骨架屏、错误 / 空态、原生 <dialog> 弹层。
 */
import { useEffect, useRef, type ReactNode } from "react";
import { ArrowUpRight, Search, X } from "lucide-react";
import { cn } from "@/lib/utils";

export function SearchField({
  value,
  onChange,
  placeholder,
  label,
  clearLabel,
}: {
  value: string;
  onChange: (value: string) => void;
  placeholder: string;
  label: string;
  clearLabel: string;
}) {
  return (
    <div className="relative w-full sm:max-w-sm">
      <Search
        size={15}
        aria-hidden
        className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-subtle"
      />
      <input
        type="search"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        aria-label={label}
        className="field pl-9 pr-9 [&::-webkit-search-cancel-button]:hidden"
      />
      {value && (
        <button
          type="button"
          onClick={() => onChange("")}
          aria-label={clearLabel}
          className="absolute right-2 top-1/2 grid h-6 w-6 -translate-y-1/2 place-items-center rounded-md text-subtle transition-colors hover:bg-inset hover:text-fg focus-visible:outline-2 focus-visible:outline-accent"
        >
          <X size={13} />
        </button>
      )}
    </div>
  );
}

/**
 * `.cells` 网格末行不满时,空槽会露出分隔线底色;按 2 列(sm)/ 3 列(lg)各自补足剩余格,
 * 以斜线阴影填充,保持蓝图网格完整。
 */
export function GridFill({ count }: { count: number }) {
  const needSm = (2 - (count % 2)) % 2;
  const needLg = (3 - (count % 3)) % 3;
  return Array.from({ length: Math.max(needSm, needLg) }, (_, i) => (
    <div
      key={`fill-${i}`}
      aria-hidden
      className={cn(
        "hidden bg-[repeating-linear-gradient(-45deg,var(--hatch)_0_1px,transparent_1px_7px)]",
        i < needSm && "sm:block",
        i < needLg ? "lg:block" : "lg:hidden",
      )}
    />
  ));
}

/** 与卡片网格同构的骨架屏,避免加载完成时布局跳动。 */
export function SkeletonGrid({ media = false }: { media?: boolean }) {
  return (
    <div className="cells grid-cols-1 sm:grid-cols-2 lg:grid-cols-3" aria-hidden>
      {Array.from({ length: 6 }, (_, i) => (
        <div key={i} className="cell flex flex-col gap-4">
          {media && <div className="aspect-[16/10] animate-pulse bg-inset" />}
          <div className="flex items-center gap-3">
            <div className="h-9 w-9 animate-pulse rounded-md bg-inset" />
            <div className="flex-1 space-y-2">
              <div className="h-3.5 w-2/3 animate-pulse rounded bg-inset" />
              <div className="h-3 w-1/3 animate-pulse rounded bg-sunken" />
            </div>
          </div>
          <div className="space-y-2">
            <div className="h-3 w-full animate-pulse rounded bg-sunken" />
            <div className="h-3 w-4/5 animate-pulse rounded bg-sunken" />
          </div>
        </div>
      ))}
    </div>
  );
}

export function StateMessage({
  tone = "muted",
  title,
  hint,
  link,
}: {
  tone?: "muted" | "danger";
  title: string;
  hint?: string;
  link?: { href: string; label: string };
}) {
  return (
    <div
      role={tone === "danger" ? "alert" : "status"}
      className="flex flex-col items-center gap-3 border-y border-line px-6 py-20 text-center"
    >
      <span className={cn("font-mono text-[11px] uppercase tracking-[0.14em]", tone === "danger" ? "text-danger" : "text-subtle")}>
        {tone === "danger" ? "[ error ]" : "[ 0 ]"}
      </span>
      <p className="text-sm text-muted">{title}</p>
      {hint && <p className="text-xs text-subtle">{hint}</p>}
      {link && (
        <a href={link.href} target="_blank" rel="noopener noreferrer" className="link inline-flex items-center gap-1 font-mono text-xs">
          {link.label}
          <ArrowUpRight size={12} />
        </a>
      )}
    </div>
  );
}

/**
 * 受控原生模态框:挂载即 showModal(),Esc / 点击遮罩 / 关闭按钮统一回调 onClose。
 * 打开期间锁定页面滚动;焦点管理与焦点陷阱由浏览器原生 <dialog> 负责。
 */
export function MarketDialog({
  onClose,
  labelledBy,
  className,
  children,
}: {
  onClose: () => void;
  labelledBy: string;
  className?: string;
  children: ReactNode;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;

  useEffect(() => {
    const dialog = ref.current;
    if (!dialog) return;
    if (!dialog.open) dialog.showModal();
    const root = document.documentElement;
    const prevOverflow = root.style.overflow;
    root.style.overflow = "hidden";
    return () => {
      root.style.overflow = prevOverflow;
      if (dialog.open) dialog.close();
    };
  }, []);

  return (
    <dialog
      ref={ref}
      aria-labelledby={labelledBy}
      onCancel={(e) => {
        e.preventDefault();
        onCloseRef.current();
      }}
      onClick={(e) => {
        if (e.target === e.currentTarget) onCloseRef.current();
      }}
      className={cn(
        "m-auto max-h-[calc(100dvh-32px)] w-[calc(100%-24px)] overflow-visible bg-transparent p-0 text-fg",
        "backdrop:bg-[oklch(0.12_0.01_265/0.55)] backdrop:backdrop-blur-[2px]",
        "opacity-100 transition-[opacity,translate] duration-200 ease-out starting:translate-y-2 starting:opacity-0",
        className,
      )}
    >
      {children}
    </dialog>
  );
}

export function DialogClose({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-label={label}
      className="grid h-8 w-8 shrink-0 place-items-center rounded-md text-muted transition-colors hover:bg-inset hover:text-fg focus-visible:outline-2 focus-visible:outline-accent"
    >
      <X size={16} />
    </button>
  );
}
