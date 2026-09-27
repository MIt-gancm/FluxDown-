/** 编辑器中的单个 token 行:颜色(取色 + alpha + ARGB hex)与数值(滑块 + 数字)。 */
import { useState, type Ref } from "react";
import { argbToCssRgba, argbToRgbHex, tokenArea, type TokenDescriptor } from "@/lib/theme-builder";
import type { ThemeBuilderMessages } from "@/i18n/messages/themeBuilder";
import { cn } from "@/lib/utils";
import { tokenAttrs } from "./tokens";

type TbMessages = ThemeBuilderMessages["tb"];

export const ROW = "grid items-center gap-2 border-b border-line px-3 py-2 transition-colors";
export const ROW_FOCUS = "bg-accent-soft shadow-[inset_2px_0_0_var(--accent)]";
export const MINI_INPUT =
  "h-7 w-full rounded-[5px] bg-bg px-1.5 font-mono text-[10.5px] text-fg shadow-[inset_0_0_0_1px_var(--line-strong)] outline-none transition-shadow focus:shadow-[inset_0_0_0_1px_var(--accent),0_0_0_3px_var(--accent-soft)]";

/** 可接受的 hex 草稿:#?RGB / ARGB / RRGGBB / AARRGGBB。 */
const HEX_DRAFT = /^#?([0-9a-f]{3}|[0-9a-f]{4}|[0-9a-f]{6}|[0-9a-f]{8})$/i;

function AreaBadge({ path, t }: { path: string; t: TbMessages }) {
  const area = tokenArea(path);
  return (
    <span
      title={t.areaHint}
      className={cn(
        "shrink-0 rounded-[3px] px-1 font-mono text-[9px] uppercase leading-[15px] tracking-wider",
        area === "settings" ? "bg-[color-mix(in_oklch,var(--cyan)_16%,transparent)] text-cyan" : "bg-accent-soft text-accent-ink",
      )}
    >
      {area === "settings" ? t.area.settings : t.area.downloads}
    </span>
  );
}

function RowLabel({ token, t }: { token: TokenDescriptor; t: TbMessages }) {
  return (
    <div className="min-w-0">
      <div className="flex items-center gap-1.5">
        <span className="truncate text-[12px] font-medium text-fg">{token.label}</span>
        <AreaBadge path={token.path} t={t} />
      </div>
      <code className="block truncate font-mono text-[10px] leading-tight text-subtle">{token.path}</code>
    </div>
  );
}

export function ColorTokenRow({
  token,
  value,
  onHexChange,
  onRgbChange,
  onAlphaChange,
  focused,
  rowRef,
  t,
}: {
  token: TokenDescriptor;
  value: string;
  onHexChange: (path: string, value: string) => void;
  onRgbChange: (path: string, value: string) => void;
  onAlphaChange: (path: string, alpha: number) => void;
  focused: boolean;
  rowRef: Ref<HTMLDivElement>;
  t: TbMessages;
}) {
  const alpha = Number.parseInt(value.slice(0, 2), 16);
  // 输入过程中保留草稿,仅在形成合法 hex 时提交(提交值经 normalizeHex8,与导入/导出同一规范化)。
  const [draft, setDraft] = useState<string | null>(null);

  return (
    <div
      ref={rowRef}
      data-token-row-path={token.path}
      className={cn(ROW, "grid-cols-[minmax(0,1fr)_28px_64px_84px]", focused ? ROW_FOCUS : "hover:bg-elev")}
      {...tokenAttrs(token.path)}
    >
      <RowLabel token={token} t={t} />

      <label
        className="relative h-6 w-7 cursor-pointer overflow-hidden rounded-[5px] bg-[repeating-conic-gradient(var(--bg-inset)_0_25%,var(--bg-elev)_0_50%)] bg-[length:8px_8px] shadow-[inset_0_0_0_1px_var(--line-strong)] focus-within:shadow-[inset_0_0_0_1px_var(--accent),0_0_0_3px_var(--accent-soft)]"
        title={`#${value}`}
      >
        <span className="absolute inset-0" style={{ backgroundColor: argbToCssRgba(value) }} />
        <input
          type="color"
          value={argbToRgbHex(value)}
          onChange={(e) => onRgbChange(token.path, e.target.value)}
          aria-label={t.colorPicker(token.label)}
          className="absolute inset-0 h-full w-full cursor-pointer opacity-0"
        />
      </label>

      <div className="flex items-center gap-1">
        <input
          type="range"
          min={0}
          max={255}
          value={alpha}
          onChange={(e) => onAlphaChange(token.path, Number.parseInt(e.target.value, 10))}
          aria-label={t.alpha(token.label)}
          className="h-1 w-full min-w-0 accent-[var(--accent)]"
        />
        <span className="num w-6 text-right font-mono text-[9.5px] text-subtle">{alpha}</span>
      </div>

      <input
        type="text"
        spellCheck={false}
        autoComplete="off"
        value={draft ?? value}
        onChange={(e) => {
          const next = e.target.value.trim();
          setDraft(next);
          if (HEX_DRAFT.test(next)) onHexChange(token.path, next);
        }}
        onBlur={() => setDraft(null)}
        aria-label={t.hex(token.label)}
        aria-invalid={draft !== null && !HEX_DRAFT.test(draft)}
        className={cn(MINI_INPUT, draft !== null && !HEX_DRAFT.test(draft) && "text-danger")}
      />
    </div>
  );
}

/** metric 数值行:alpha 单位步进 0.01(区间 [0,1]),其余 px 按描述符 step。 */
export function NumberTokenRow({
  token,
  value,
  onChange,
  focused,
  rowRef,
  t,
}: {
  token: TokenDescriptor;
  value: number;
  onChange: (path: string, value: number) => void;
  focused: boolean;
  rowRef: Ref<HTMLDivElement>;
  t: TbMessages;
}) {
  const isAlpha = token.unit === "alpha";
  const min = token.min ?? 0;
  const max = token.max ?? (isAlpha ? 1 : 2000);
  const step = token.step ?? (isAlpha ? 0.01 : 1);
  // 滑块上限按 px 语义收敛到可用区间(数值框仍可输入到 max),避免 0–2000 的滑块不可控。
  const sliderMax = isAlpha ? max : Math.min(max, Math.max(token.path.endsWith(".pill") ? 999 : 64, value));

  // 与 hex 输入同理:输入中保留草稿,可解析时即提交(夹取到 [min, max]),失焦恢复显示规范值。
  const [draft, setDraft] = useState<string | null>(null);
  const commit = (raw: string) => {
    setDraft(raw);
    const parsed = Number.parseFloat(raw);
    if (Number.isNaN(parsed)) return;
    onChange(token.path, Math.min(max, Math.max(min, parsed)));
  };

  return (
    <div
      ref={rowRef}
      data-token-row-path={token.path}
      className={cn(ROW, "grid-cols-[minmax(0,1fr)_minmax(0,0.9fr)_72px]", focused ? ROW_FOCUS : "hover:bg-elev")}
      {...tokenAttrs(token.path)}
    >
      <RowLabel token={token} t={t} />
      <input
        type="range"
        min={min}
        max={sliderMax}
        step={step}
        value={value}
        onChange={(e) => onChange(token.path, Number.parseFloat(e.target.value))}
        aria-label={t.numberValue(token.label)}
        className="h-1 w-full min-w-0 accent-[var(--accent)]"
      />
      <div className="flex items-center gap-1">
        <input
          type="number"
          min={min}
          max={max}
          step={step}
          value={draft ?? (isAlpha ? value.toFixed(2) : String(value))}
          onChange={(e) => commit(e.target.value)}
          onBlur={() => setDraft(null)}
          aria-label={t.numberValue(token.label)}
          className={MINI_INPUT}
        />
        <span className="w-3 font-mono text-[9.5px] text-subtle">{isAlpha ? "α" : "px"}</span>
      </div>
    </div>
  );
}
