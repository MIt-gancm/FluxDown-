/**
 * GPUI 编辑器的单个 token 行：在『继承默认 / 引用 / 字面量』间切换，按 token 类型渲染
 * 颜色 / 数值（长度、圆角、字重）/ 字体 / 阴影编辑器；kitBound token 只读。
 */
import { useState, type Ref } from "react";
import { Lock, Plus, Trash2 } from "lucide-react";
import type { TokenSpec } from "@/lib/gpui-theme/registry";
import type { Json, JsonObject, TokenValue } from "@/lib/gpui-theme/types";
import { isJsonObject } from "@/lib/gpui-theme/types";
import { colorHex, normalizedLiteral, parseHexColor, tokenValueJson } from "@/lib/gpui-theme/value";
import type { ThemeBuilderMessages } from "@/i18n/messages/themeBuilder";
import { cn } from "@/lib/utils";
import { tokenAttrs } from "../tokens";
import { MINI_INPUT, ROW, ROW_FOCUS } from "../TokenRows";
import { displayValue, refCandidates, sourceOf, splitRef, type TokenSource } from "./model";

type GpuiMessages = ThemeBuilderMessages["gpui"];

const HEX_DRAFT = /^#?([0-9a-f]{6}|[0-9a-f]{8})$/i;
const SEGMENT = "inline-flex h-6 items-center justify-center rounded-[4px] px-1.5 text-[10.5px] font-medium transition-colors focus-visible:outline-2 focus-visible:outline-accent";

function Swatch({ color }: { color: string }) {
  return (
    <span className="relative inline-block h-5 w-6 shrink-0 overflow-hidden rounded-[4px] bg-[repeating-conic-gradient(var(--bg-inset)_0_25%,var(--bg-elev)_0_50%)] bg-[length:8px_8px] shadow-[inset_0_0_0_1px_var(--line-strong)]">
      <span className="absolute inset-0" style={{ backgroundColor: color }} />
    </span>
  );
}

/** `#rrggbbaa` 颜色编辑：取色（RGB）+ alpha 滑块 + hex 文本。 */
function ColorEditor({
  value,
  label,
  onChange,
  t,
}: {
  value: string;
  label: string;
  onChange: (hex: string) => void;
  t: GpuiMessages;
}) {
  const [draft, setDraft] = useState<string | null>(null);
  const rgb = value.slice(0, 7);
  const alpha = Number.parseInt(value.slice(7, 9) || "ff", 16);
  const commitDraft = (next: string) => {
    setDraft(next);
    if (!HEX_DRAFT.test(next)) return;
    const hex = next.startsWith("#") ? next : `#${next}`;
    const normalized = normalizedLiteral("color", hex);
    if (typeof normalized === "string") onChange(normalized);
  };
  return (
    <div className="grid grid-cols-[28px_minmax(0,1fr)_92px] items-center gap-2">
      <label className="relative h-6 w-7 cursor-pointer" title={value}>
        <Swatch color={value} />
        <input
          type="color"
          value={rgb}
          onChange={(e) => onChange(`${e.target.value.toLowerCase()}${value.slice(7, 9) || "ff"}`)}
          aria-label={t.colorPicker(label)}
          className="absolute inset-0 h-full w-full cursor-pointer opacity-0"
        />
      </label>
      <div className="flex items-center gap-1">
        <input
          type="range"
          min={0}
          max={255}
          value={alpha}
          onChange={(e) => onChange(`${rgb}${Number(e.target.value).toString(16).padStart(2, "0")}`)}
          aria-label={t.alpha(label)}
          className="h-1 w-full min-w-0 accent-[var(--accent)]"
        />
        <span className="num w-6 text-right font-mono text-[9.5px] text-subtle">{alpha}</span>
      </div>
      <input
        type="text"
        spellCheck={false}
        autoComplete="off"
        value={draft ?? value}
        onChange={(e) => commitDraft(e.target.value.trim())}
        onBlur={() => setDraft(null)}
        aria-label={t.hex(label)}
        aria-invalid={draft !== null && !HEX_DRAFT.test(draft)}
        className={cn(MINI_INPUT, draft !== null && !HEX_DRAFT.test(draft) && "text-danger")}
      />
    </div>
  );
}

function NumberEditor({
  spec,
  value,
  label,
  onChange,
  t,
}: {
  spec: TokenSpec;
  value: number;
  label: string;
  onChange: (value: number) => void;
  t: GpuiMessages;
}) {
  const [draft, setDraft] = useState<string | null>(null);
  const min = spec.range?.min ?? 0;
  const max = spec.range?.max ?? 999;
  const step = spec.kind === "fontWeight" ? 100 : 1;
  // radius.full 等上限 9999 的滑块收敛到可控区间，数字框仍可输入到上限。
  const sliderMax = Math.min(max, Math.max(64, value));
  const unit = spec.kind === "fontWeight" ? "wt" : "px";
  return (
    <div className="grid grid-cols-[minmax(0,1fr)_92px] items-center gap-2">
      <input
        type="range"
        min={min}
        max={sliderMax}
        step={step}
        value={value}
        onChange={(e) => onChange(Number.parseFloat(e.target.value))}
        aria-label={t.numberValue(label)}
        className="h-1 w-full min-w-0 accent-[var(--accent)]"
      />
      <div className="flex items-center gap-1">
        <input
          type="number"
          min={min}
          max={max}
          step={step}
          value={draft ?? String(value)}
          onChange={(e) => {
            setDraft(e.target.value);
            const parsed = Number.parseFloat(e.target.value);
            if (!Number.isNaN(parsed)) onChange(Math.min(max, Math.max(min, parsed)));
          }}
          onBlur={() => setDraft(null)}
          aria-label={t.numberValue(label)}
          className={MINI_INPUT}
        />
        <span className="w-4 font-mono text-[9.5px] text-subtle">{unit}</span>
      </div>
    </div>
  );
}

function FontEditor({ value, label, onChange, t }: { value: string; label: string; onChange: (value: string) => void; t: GpuiMessages }) {
  const [draft, setDraft] = useState<string | null>(null);
  return (
    <input
      type="text"
      spellCheck={false}
      value={draft ?? value}
      onChange={(e) => {
        setDraft(e.target.value);
        if (e.target.value.trim()) onChange(e.target.value);
      }}
      onBlur={() => setDraft(null)}
      aria-label={t.font(label)}
      className={cn(MINI_INPUT, "text-[11px]")}
    />
  );
}

const SHADOW_FIELDS = ["x", "y", "blur", "spread"] as const;

/** 阴影层列表：x / y / blur / spread / color / inset，可增删层。颜色可为引用串，原样保留。 */
function ShadowEditor({ value, onChange, t }: { value: JsonObject[]; onChange: (layers: JsonObject[]) => void; t: GpuiMessages }) {
  const update = (index: number, patch: JsonObject) =>
    onChange(value.map((layer, i) => (i === index ? { ...layer, ...patch } : layer)));
  return (
    <div className="space-y-1.5">
      {value.length === 0 && <p className="text-[10.5px] text-subtle">{t.shadow.none}</p>}
      {value.map((layer, index) => {
        const color = typeof layer.color === "string" ? layer.color : "#0000002e";
        const literal = parseHexColor(color);
        return (
          <div key={index} className="rounded-[5px] p-1.5 shadow-[inset_0_0_0_1px_var(--line)]">
            <div className="mb-1 flex items-center justify-between font-mono text-[9.5px] uppercase tracking-wider text-subtle">
              <span>{t.shadow.layer(index + 1)}</span>
              <span className="flex items-center gap-2">
                <label className="flex items-center gap-1 normal-case tracking-normal">
                  <input
                    type="checkbox"
                    checked={layer.inset === true}
                    onChange={(e) => update(index, { inset: e.target.checked })}
                    className="accent-[var(--accent)]"
                  />
                  {t.shadow.inset}
                </label>
                <button
                  type="button"
                  onClick={() => onChange(value.filter((_, i) => i !== index))}
                  aria-label={t.shadow.remove}
                  title={t.shadow.remove}
                  className="grid h-5 w-5 place-items-center rounded text-subtle hover:bg-inset hover:text-danger"
                >
                  <Trash2 size={11} />
                </button>
              </span>
            </div>
            <div className="grid grid-cols-[repeat(4,minmax(0,1fr))_minmax(0,1.6fr)] gap-1">
              {SHADOW_FIELDS.map((field) => (
                <label key={field} className="block">
                  <span className="block font-mono text-[9px] text-subtle">{field}</span>
                  <input
                    type="number"
                    step={1}
                    value={typeof layer[field] === "number" ? String(layer[field]) : "0"}
                    onChange={(e) => {
                      const parsed = Number.parseFloat(e.target.value);
                      if (!Number.isNaN(parsed)) update(index, { [field]: parsed });
                    }}
                    className={MINI_INPUT}
                  />
                </label>
              ))}
              <label className="block">
                <span className="flex items-center gap-1 font-mono text-[9px] text-subtle">
                  color {literal && <Swatch color={colorHex(literal)} />}
                </span>
                <input
                  type="text"
                  spellCheck={false}
                  defaultValue={color}
                  onBlur={(e) => update(index, { color: e.target.value.trim() })}
                  className={MINI_INPUT}
                />
              </label>
            </div>
          </div>
        );
      })}
      <button
        type="button"
        onClick={() => onChange([...value, { x: 0, y: 1, blur: 2, spread: 0, color: "#0000002e", inset: false }])}
        className="inline-flex h-6 items-center gap-1 rounded-[4px] px-1.5 text-[10.5px] text-muted shadow-[inset_0_0_0_1px_var(--line-strong)] hover:bg-inset hover:text-fg"
      >
        <Plus size={11} aria-hidden />
        {t.shadow.add}
      </button>
    </div>
  );
}

function RefEditor({
  spec,
  raw,
  label,
  onChange,
  t,
}: {
  spec: TokenSpec;
  raw: string;
  label: string;
  onChange: (value: string) => void;
  t: GpuiMessages;
}) {
  const parts = splitRef(raw);
  const candidates = refCandidates(spec);
  const target = parts?.path ?? "";
  const build = (path: string, opacity: number | undefined) =>
    opacity === undefined ? `{${path}}` : `{${path}}/${opacity}`;
  return (
    <div className={cn("grid items-center gap-2", spec.kind === "color" ? "grid-cols-[minmax(0,1fr)_76px]" : "grid-cols-1")}>
      <select
        value={candidates.includes(target) ? target : ""}
        onChange={(e) => onChange(build(e.target.value, parts?.opacity))}
        aria-label={t.refTarget(label)}
        className={cn(MINI_INPUT, "pr-5")}
      >
        {!candidates.includes(target) && <option value="">{raw}</option>}
        {candidates.map((path) => (
          <option key={path} value={path}>
            {path}
          </option>
        ))}
      </select>
      {spec.kind === "color" && (
        <label className="flex items-center gap-1">
          <input
            type="number"
            min={0}
            max={100}
            step={1}
            placeholder="100"
            value={parts?.opacity ?? ""}
            onChange={(e) => {
              const parsed = Number.parseFloat(e.target.value);
              onChange(build(target || candidates[0], Number.isNaN(parsed) ? undefined : Math.min(100, Math.max(0, parsed))));
            }}
            aria-label={t.refOpacity}
            title={t.refOpacity}
            className={MINI_INPUT}
          />
          <span className="font-mono text-[9.5px] text-subtle">%</span>
        </label>
      )}
    </div>
  );
}

/** 切到『引用』时的默认目标：注册表默认表达式引用的 token，否则第一个兼容 token。 */
function defaultRef(spec: TokenSpec): string {
  const expr = spec.default;
  if (expr.kind === "ref" || expr.kind === "refAlpha" || expr.kind === "refOffset") return `{${expr.path}}`;
  if (expr.kind === "mix") return `{${expr.from}}`;
  return `{${refCandidates(spec)[0] ?? spec.path}}`;
}

export function GpuiTokenRow({
  spec,
  raw,
  resolved,
  focused,
  rowRef,
  onSet,
  onRemove,
  t,
}: {
  spec: TokenSpec;
  /** 当前编辑层中的原始值（`undefined` = 继承）。 */
  raw: Json | undefined;
  /** 当前层所对应模式下的解析结果。 */
  resolved: TokenValue | undefined;
  focused: boolean;
  rowRef: Ref<HTMLDivElement>;
  onSet: (path: string, value: Json) => void;
  onRemove: (path: string) => void;
  t: GpuiMessages;
}) {
  const label = spec.path.slice(spec.group.length + 1);
  const source = sourceOf(raw);
  const resolvedJson = resolved ? tokenValueJson(resolved) : null;

  const switchSource = (next: TokenSource) => {
    if (next === source) return;
    if (next === "inherit") onRemove(spec.path);
    else if (next === "ref") onSet(spec.path, defaultRef(spec));
    else if (resolvedJson !== null) onSet(spec.path, resolvedJson);
  };

  const literalEditor = () => {
    switch (spec.kind) {
      case "color":
        return (
          <ColorEditor
            value={typeof raw === "string" && parseHexColor(raw) ? String(normalizedLiteral("color", raw)) : String(resolvedJson)}
            label={label}
            onChange={(hex) => onSet(spec.path, hex)}
            t={t}
          />
        );
      case "fontFamily":
        return <FontEditor value={typeof raw === "string" ? raw : String(resolvedJson)} label={label} onChange={(font) => onSet(spec.path, font)} t={t} />;
      case "shadow": {
        const layers = Array.isArray(raw) ? raw.filter(isJsonObject) : Array.isArray(resolvedJson) ? resolvedJson.filter(isJsonObject) : [];
        return <ShadowEditor value={layers} onChange={(next) => onSet(spec.path, next)} t={t} />;
      }
      default:
        return (
          <NumberEditor
            spec={spec}
            value={typeof raw === "number" ? raw : typeof resolvedJson === "number" ? resolvedJson : 0}
            label={label}
            onChange={(number) => onSet(spec.path, number)}
            t={t}
          />
        );
    }
  };

  const colorPreview = resolved?.type === "color" ? colorHex(resolved.color) : null;

  return (
    <div
      ref={rowRef}
      data-token-row-path={spec.path}
      className={cn(ROW, "grid-cols-1 gap-1.5", focused ? ROW_FOCUS : "hover:bg-elev")}
      {...tokenAttrs(spec.path)}
    >
      <div className="flex items-center gap-2">
        {colorPreview && <Swatch color={colorPreview} />}
        <div className="min-w-0 flex-1">
          <span className="block truncate text-[12px] font-medium text-fg">{label}</span>
          <code className="block truncate font-mono text-[10px] leading-tight text-subtle">{spec.path}</code>
        </div>
        {spec.kitBound ? (
          <span
            title={t.kitBound}
            className="inline-flex shrink-0 items-center gap-1 rounded-[3px] bg-[color-mix(in_oklch,var(--cyan)_16%,transparent)] px-1 font-mono text-[9px] uppercase leading-[15px] tracking-wider text-cyan"
          >
            <Lock size={9} aria-hidden />
            {t.readOnly}
          </span>
        ) : (
          <div role="group" aria-label={t.source.label} className="flex shrink-0 rounded-[5px] p-0.5 shadow-[inset_0_0_0_1px_var(--line-strong)]">
            {(["inherit", "ref", "literal"] as const).map((option) => (
              <button
                key={option}
                type="button"
                aria-pressed={source === option}
                onClick={() => switchSource(option)}
                className={cn(SEGMENT, source === option ? "bg-accent-soft text-accent-ink" : "text-muted hover:text-fg")}
              >
                {t.source[option]}
              </button>
            ))}
          </div>
        )}
      </div>

      {spec.kitBound ? (
        <p className="text-[10.5px] leading-snug text-subtle">
          {t.kitBound} · <code className="font-mono">{displayValue(resolved)}</code>
        </p>
      ) : source === "inherit" ? (
        <p className="truncate font-mono text-[10.5px] text-subtle" title={displayValue(resolved)}>
          {t.inherited(displayValue(resolved))}
        </p>
      ) : source === "ref" ? (
        <RefEditor spec={spec} raw={String(raw)} label={label} onChange={(next) => onSet(spec.path, next)} t={t} />
      ) : (
        literalEditor()
      )}
    </div>
  );
}
