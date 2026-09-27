/**
 * GPUI 桌面客户端模拟：复刻 `crates/shell`（标题栏 + 活动栏）与 `crates/downloads`
 * （侧栏『状态』文件夹内嵌分类子项、无网格线任务表、浮动选择条、状态栏、新建下载对话框）。
 *
 * 所有颜色 / 尺寸 / 圆角 / 字体 / 阴影都只读根节点上的 CSS 变量（由 resolve 结果生成，
 * 见 `model.ts`），元素挂 `data-token-paths` 供右键检视。
 */
import { useState, type CSSProperties, type ReactNode } from "react";
import {
  ArrowDownToLine,
  ArrowUp,
  Check,
  ChevronDown,
  ChevronRight,
  CircleAlert,
  CircleArrowDown,
  CircleCheck,
  CirclePause,
  Clock,
  File,
  FileArchive,
  FileAudio,
  FileImage,
  FileText,
  FileVideo,
  Folder,
  HardDrive,
  Layers,
  Minus,
  Moon,
  Pause,
  Play,
  Plus,
  Puzzle,
  Rss,
  Search,
  Settings,
  SlidersHorizontal,
  Square,
  Sun,
  Trash2,
  X,
} from "lucide-react";
import type { ResolvedTokens } from "@/lib/gpui-theme/resolve";
import type { ThemeMode } from "@/lib/gpui-theme/types";
import type { ThemeBuilderMessages } from "@/i18n/messages/themeBuilder";
import { cn } from "@/lib/utils";
import { tokenAttrs } from "../tokens";
import { cssVariables, v } from "./model";

type PreviewMessages = ThemeBuilderMessages["gpuiMock"];

type Status = "downloading" | "paused" | "failed" | "queued" | "completed";
type Category = "video" | "audio" | "document" | "image" | "archive" | "other";
type StatusFolder = "all" | "downloading" | "completed" | "failed" | "paused";

interface DemoTask {
  id: string;
  name: string;
  category: Category;
  size: string;
  progress: number;
  speed: string;
  status: Status;
  created: string;
}

const TASKS: DemoTask[] = [
  { id: "1", name: "ubuntu-24.04.1-desktop-amd64.iso", category: "archive", size: "5.7 GB", progress: 0.62, speed: "11.4 MB/s", status: "downloading", created: "2026-09-27 10:42" },
  { id: "2", name: "Big.Buck.Bunny.2160p.mkv", category: "video", size: "2.1 GB", progress: 0.35, speed: "1.4 MB/s", status: "downloading", created: "2026-09-27 10:31" },
  { id: "3", name: "annual-report-2025.pdf", category: "document", size: "18.4 MB", progress: 0.48, speed: "", status: "paused", created: "2026-09-26 21:07" },
  { id: "4", name: "lofi-mix-vol3.flac", category: "audio", size: "412 MB", progress: 0.12, speed: "", status: "failed", created: "2026-09-26 18:55" },
  { id: "5", name: "wallpaper-pack-4k.zip", category: "image", size: "1.3 GB", progress: 0, speed: "", status: "queued", created: "2026-09-26 18:40" },
  { id: "6", name: "node-v22.9.0-x64.msi", category: "other", size: "28.9 MB", progress: 1, speed: "", status: "completed", created: "2026-09-25 09:12" },
];

const CATEGORY_ICON: Record<Category, typeof File> = {
  video: FileVideo,
  audio: FileAudio,
  document: FileText,
  image: FileImage,
  archive: FileArchive,
  other: File,
};

const CATEGORIES: Category[] = ["video", "audio", "document", "image", "archive", "other"];

/** 状态文字色（task_table.rs `status_color`）。 */
const STATUS_TEXT: Record<Status, string> = {
  downloading: "colors.statusDownloading",
  paused: "colors.mutedForeground",
  failed: "colors.statusFailed",
  queued: "colors.textTertiary",
  completed: "colors.textTertiary",
};

/** 进度条填充色（task_table.rs `progress_bar_color`）；暂停为 statusPaused 的 40%。 */
const PROGRESS_FILL: Record<Status, { path: string; css: string }> = {
  downloading: { path: "colors.progressFill", css: v("colors.progressFill") },
  paused: { path: "colors.statusPaused", css: `color-mix(in srgb, ${v("colors.statusPaused")} 40%, transparent)` },
  failed: { path: "colors.statusFailed", css: v("colors.statusFailed") },
  queued: { path: "colors.statusQueued", css: v("colors.statusQueued") },
  completed: { path: "colors.statusCompleted", css: v("colors.statusCompleted") },
};

const FOLDER_MATCH: Record<StatusFolder, (task: DemoTask) => boolean> = {
  all: () => true,
  downloading: (task) => task.status === "downloading" || task.status === "queued",
  completed: (task) => task.status === "completed",
  failed: (task) => task.status === "failed",
  paused: (task) => task.status === "paused",
};

const FOLDER_ICON: Record<StatusFolder, typeof Layers> = {
  all: Layers,
  downloading: CircleArrowDown,
  completed: CircleCheck,
  failed: CircleAlert,
  paused: CirclePause,
};

const FOLDERS: StatusFolder[] = ["all", "downloading", "completed", "failed", "paused"];

/** 文字角色（`typography.<role>.{size,lineHeight,weight}`）。 */
function text(role: string): CSSProperties {
  return {
    fontSize: v(`typography.${role}.size`),
    lineHeight: v(`typography.${role}.lineHeight`),
    fontWeight: v(`typography.${role}.weight`),
  };
}

const hairline = `${v("stroke.thin")} solid ${v("colors.hairline")}`;

function CheckMark({ checked }: { checked: boolean }) {
  return (
    <span
      className="grid shrink-0 place-items-center"
      style={{
        width: v("density.checkMark"),
        height: v("density.checkMark"),
        borderRadius: v("components.checkbox.radius"),
        backgroundColor: checked ? v("colors.primary") : "transparent",
        border: checked ? "none" : `${v("stroke.thin")} solid ${v("colors.border")}`,
        color: v("colors.primaryForeground"),
      }}
      {...tokenAttrs("density.checkMark", "components.checkbox.radius", "colors.primary", "colors.primaryForeground", "colors.border")}
    >
      {checked && <Check style={{ width: v("icon.sm"), height: v("icon.sm") }} strokeWidth={3} />}
    </span>
  );
}

function ProgressBar({ task }: { task: DemoTask }) {
  const fill = PROGRESS_FILL[task.status];
  return (
    <div
      className="w-full overflow-hidden"
      style={{ height: v("components.progress.height"), borderRadius: v("components.progress.radius"), backgroundColor: v("colors.progressTrack") }}
      {...tokenAttrs("components.progress.height", "components.progress.radius", "colors.progressTrack", fill.path)}
    >
      <div className="h-full" style={{ width: `${task.progress * 100}%`, backgroundColor: fill.css, borderRadius: v("components.progress.radius") }} />
    </div>
  );
}

function Button({
  variant,
  children,
  onClick,
  paths = [],
}: {
  variant: "primary" | "secondary" | "ghost";
  children: ReactNode;
  onClick?: () => void;
  paths?: string[];
}) {
  const colors: Record<typeof variant, { style: CSSProperties; paths: string[] }> = {
    primary: {
      style: { backgroundColor: v("colors.primary"), color: v("colors.primaryForeground") },
      paths: ["colors.primary", "colors.primaryForeground"],
    },
    secondary: {
      style: { backgroundColor: v("colors.secondary"), color: v("colors.secondaryForeground"), border: `${v("stroke.thin")} solid ${v("colors.border")}` },
      paths: ["colors.secondary", "colors.secondaryForeground", "colors.border"],
    },
    ghost: { style: { color: v("colors.foreground") }, paths: ["colors.foreground", "colors.navHover"] },
  };
  return (
    <button
      type="button"
      onClick={onClick}
      className="inline-flex shrink-0 items-center hover:brightness-110"
      style={{
        height: v("density.toolbarButton"),
        paddingInline: v("spacing.md"),
        gap: v("spacing.xs"),
        borderRadius: v("components.button.radius"),
        ...text("sm"),
        ...colors[variant].style,
      }}
      {...tokenAttrs("components.button.radius", "density.toolbarButton", "spacing.md", ...colors[variant].paths, ...paths)}
    >
      {children}
    </button>
  );
}

function Switch({ on }: { on: boolean }) {
  return (
    <span
      className="relative inline-block h-[18px] w-8 shrink-0"
      style={{ borderRadius: v("radius.full"), backgroundColor: on ? v("colors.primary") : v("colors.input") }}
      {...tokenAttrs("colors.primary", "colors.input", "radius.full", "colors.background")}
    >
      <span
        className="absolute top-[2px] h-[14px] w-[14px] transition-[left]"
        style={{ left: on ? 16 : 2, borderRadius: v("radius.full"), backgroundColor: v("colors.background"), boxShadow: v("shadow.sm") }}
      />
    </span>
  );
}

function Badge({ children, tone }: { children: ReactNode; tone: "primary" | "muted" | "success" | "warning" }) {
  const map = {
    primary: ["colors.accent", "colors.accentForeground"],
    muted: ["colors.muted", "colors.mutedForeground"],
    success: ["colors.success", "colors.background"],
    warning: ["colors.warning", "colors.background"],
  } as const;
  const [bg, fg] = map[tone];
  return (
    <span
      className="inline-flex items-center"
      style={{ ...text("caption"), backgroundColor: v(bg), color: v(fg), borderRadius: v("components.badge.radius"), paddingInline: v("spacing.sm") }}
      {...tokenAttrs("components.badge.radius", bg, fg, "typography.caption.size")}
    >
      {children}
    </span>
  );
}

function Input({ value, focused, children }: { value: string; focused?: boolean; children?: ReactNode }) {
  return (
    <div
      className="flex min-w-0 flex-1 items-center"
      style={{
        height: v("density.control"),
        paddingInline: v("spacing.sm"),
        gap: v("spacing.xs"),
        borderRadius: v("components.input.radius"),
        backgroundColor: v("colors.background"),
        border: `${v("stroke.thin")} solid ${focused ? v("colors.ring") : v("colors.input")}`,
        boxShadow: focused ? `0 0 0 ${v("focusRing.width")} color-mix(in srgb, ${v("colors.ring")} 30%, transparent)` : undefined,
        ...text("sm"),
      }}
      {...tokenAttrs("components.input.radius", "density.control", "colors.input", "colors.background", ...(focused ? ["colors.ring", "focusRing.width"] : []))}
    >
      <span className="min-w-0 flex-1 truncate">{value}</span>
      {children}
    </div>
  );
}

function NewDownloadDialog({ t, onClose }: { t: PreviewMessages; onClose: () => void }) {
  const [tab, setTab] = useState<"basic" | "advanced">("basic");
  const [remember, setRemember] = useState(true);
  return (
    <div
      className="absolute inset-0 z-20 grid place-items-center p-6"
      style={{ backgroundColor: "rgba(0, 0, 0, 0.36)" }}
      onClick={onClose}
    >
      <div
        role="dialog"
        aria-label={t.dialogTitle}
        className="flex w-full max-w-[440px] flex-col"
        style={{
          backgroundColor: v("colors.surface"),
          color: v("colors.surfaceForeground"),
          borderRadius: v("components.dialog.radius"),
          border: `${v("stroke.thin")} solid ${v("colors.border")}`,
          boxShadow: v("shadow.lg"),
          padding: v("spacing.lg"),
          gap: v("spacing.md"),
        }}
        onClick={(e) => e.stopPropagation()}
        {...tokenAttrs("components.dialog.radius", "colors.surface", "colors.surfaceForeground", "colors.border", "shadow.lg", "spacing.lg")}
      >
        <div className="flex items-center justify-between">
          <span style={text("title")} {...tokenAttrs("typography.title.size", "typography.title.weight")}>
            {t.dialogTitle}
          </span>
          <button type="button" onClick={onClose} aria-label={t.cancel} style={{ color: v("colors.mutedForeground") }}>
            <X style={{ width: v("icon.md"), height: v("icon.md") }} />
          </button>
        </div>

        <div
          className="flex p-0.5"
          style={{ backgroundColor: v("colors.muted"), borderRadius: v("components.tab.radius"), gap: 2 }}
          {...tokenAttrs("components.tab.radius", "colors.muted")}
        >
          {(["basic", "advanced"] as const).map((key) => (
            <button
              key={key}
              type="button"
              onClick={() => setTab(key)}
              className="flex-1"
              style={{
                ...text("sm"),
                height: `calc(${v("density.control")} - 4px)`,
                borderRadius: v("components.tab.radius"),
                backgroundColor: tab === key ? v("colors.background") : "transparent",
                color: tab === key ? v("colors.foreground") : v("colors.mutedForeground"),
                boxShadow: tab === key ? v("shadow.sm") : undefined,
              }}
              {...tokenAttrs("components.tab.radius", "colors.background", "colors.mutedForeground", "shadow.sm")}
            >
              {key === "basic" ? t.tabBasic : t.tabAdvanced}
            </button>
          ))}
        </div>

        <label className="flex flex-col" style={{ gap: v("spacing.xs") }}>
          <span style={{ ...text("caption"), color: v("colors.mutedForeground") }}>{t.fieldUrl}</span>
          <Input value="https://releases.ubuntu.com/24.04/ubuntu-24.04.1-desktop-amd64.iso" focused />
        </label>
        <label className="flex flex-col" style={{ gap: v("spacing.xs") }}>
          <span style={{ ...text("caption"), color: v("colors.mutedForeground") }}>{t.fieldSaveDir}</span>
          <div className="flex" style={{ gap: v("spacing.xs") }}>
            <Input value="~/Downloads" />
            <Button variant="secondary">
              <Folder style={{ width: v("icon.md"), height: v("icon.md") }} />
              {t.browse}
            </Button>
          </div>
        </label>
        <div className="flex flex-wrap items-center" style={{ gap: v("spacing.xs") }}>
          <Badge tone="primary">HTTPS</Badge>
          <Badge tone="muted">{t.threads(16)}</Badge>
          <Badge tone="success">{t.resumable}</Badge>
          <Badge tone="warning">{t.largeFile}</Badge>
        </div>
        <button
          type="button"
          onClick={() => setRemember((value) => !value)}
          className="flex items-center justify-between text-left"
          style={{ ...text("sm"), paddingBlock: v("spacing.xs") }}
        >
          <span>{t.rememberDir}</span>
          <Switch on={remember} />
        </button>
        <div className="flex items-center justify-between" style={{ ...text("sm"), color: v("colors.mutedForeground") }}>
          <span>{t.startPaused}</span>
          <Switch on={false} />
        </div>

        <div className="flex justify-end" style={{ gap: v("spacing.sm"), paddingTop: v("spacing.xs") }}>
          <Button variant="secondary" onClick={onClose}>
            {t.cancel}
          </Button>
          <Button variant="primary" onClick={onClose}>
            <ArrowDownToLine style={{ width: v("icon.md"), height: v("icon.md") }} />
            {t.startDownload}
          </Button>
        </div>
      </div>
    </div>
  );
}

export function GpuiPreview({
  tokens,
  mode,
  t,
  onToggleMode,
}: {
  tokens: ResolvedTokens;
  mode: ThemeMode;
  t: PreviewMessages;
  onToggleMode: () => void;
}) {
  const [folder, setFolder] = useState<StatusFolder>("all");
  const [category, setCategory] = useState<Category | null>(null);
  const [expanded, setExpanded] = useState<StatusFolder>("all");
  const [selected, setSelected] = useState<Set<string>>(() => new Set(["2", "3"]));
  const [dialog, setDialog] = useState(false);

  const visible = TASKS.filter((task) => FOLDER_MATCH[folder](task) && (category === null || task.category === category));
  const failedCount = TASKS.filter((task) => task.status === "failed").length;
  const ModeIcon = mode === "dark" ? Sun : Moon;
  const iconMd = { width: v("icon.md"), height: v("icon.md") };
  const iconSm = { width: v("icon.sm"), height: v("icon.sm") };

  const toggleSelected = (id: string) =>
    setSelected((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  return (
    <div
      className="@container relative flex h-full min-h-0 flex-col overflow-hidden"
      style={{
        ...cssVariables(tokens),
        backgroundColor: v("colors.background"),
        color: v("colors.foreground"),
        fontFamily: v("typography.sans"),
        border: `${v("stroke.thin")} solid ${v("colors.border")}`,
        borderRadius: v("radius.lg"),
        boxShadow: v("shadow.lg"),
        ...text("sm"),
      }}
      {...tokenAttrs("colors.background", "colors.foreground", "typography.sans", "colors.border", "radius.lg", "shadow.lg")}
    >
      {/* 标题栏：crates/shell view.rs + downloads title_bar.rs */}
      <div
        className="flex shrink-0 items-center"
        style={{ height: v("density.titleBar"), backgroundColor: v("colors.chrome"), borderBottom: hairline, paddingLeft: v("spacing.md"), gap: v("spacing.md") }}
        {...tokenAttrs("density.titleBar", "colors.chrome", "colors.hairline", "stroke.thin")}
      >
        <div className="flex shrink-0 items-center" style={{ gap: v("spacing.sm") }}>
          <img src="/logo.svg" alt="" className="h-4 w-4" />
          <span style={{ ...text("sm"), fontWeight: v("typography.title.weight") }}>FluxDown</span>
        </div>
        <div className="flex min-w-0 flex-1 justify-center">
          <div
            className="flex w-full max-w-[280px] min-w-[120px] items-center"
            style={{
              height: v("density.control"),
              borderRadius: v("radius.md"),
              backgroundColor: v("colors.navHover"),
              color: v("colors.mutedForeground"),
              paddingInline: v("spacing.sm"),
              gap: v("spacing.xs"),
              ...text("sm"),
            }}
            {...tokenAttrs("density.control", "radius.md", "colors.navHover", "colors.mutedForeground")}
          >
            <Search style={iconMd} />
            <span className="min-w-0 flex-1 truncate">{t.searchPlaceholder}</span>
            <span
              className="hidden @2xl:inline"
              style={{ ...text("caption"), fontFamily: v("typography.mono"), color: v("colors.textTertiary"), borderRadius: v("radius.sm"), border: hairline, paddingInline: v("spacing.xxs") }}
              {...tokenAttrs("typography.mono", "colors.textTertiary", "radius.sm")}
            >
              Ctrl K
            </span>
          </div>
        </div>
        <div className="flex shrink-0 items-center" style={{ gap: v("spacing.xs") }}>
          <Button variant="ghost">
            <SlidersHorizontal style={iconMd} />
            <span className="hidden @3xl:inline">{t.view}</span>
          </Button>
          <Button variant="primary" onClick={() => setDialog(true)}>
            <Plus style={iconMd} />
            {t.newDownload}
          </Button>
        </div>
        <div className="flex shrink-0 items-center self-stretch" style={{ color: v("colors.mutedForeground") }} {...tokenAttrs("colors.mutedForeground")}>
          {[Minus, Square, X].map((Icon, index) => (
            <span key={index} className="grid h-full w-10 place-items-center" aria-hidden>
              <Icon style={index === 1 ? iconSm : iconMd} />
            </span>
          ))}
        </div>
      </div>

      <div className="flex min-h-0 flex-1">
        {/* 活动栏：48px，chrome 底 */}
        <div
          className="flex w-12 shrink-0 flex-col items-center justify-between"
          style={{ backgroundColor: v("colors.chrome"), borderRight: hairline, paddingBlock: v("spacing.sm") }}
          {...tokenAttrs("colors.chrome", "colors.hairline")}
        >
          <div className="flex flex-col" style={{ gap: v("spacing.xs") }}>
            {[ArrowDownToLine, Rss, Puzzle].map((Icon, index) => (
              <span
                key={index}
                className="grid h-8 w-8 place-items-center hover:bg-[var(--gt-colors-navHover)]"
                style={{
                  borderRadius: v("components.navItem.radius"),
                  backgroundColor: index === 0 ? v("colors.navSelected") : undefined,
                  color: index === 0 ? v("colors.foreground") : v("colors.mutedForeground"),
                }}
                {...tokenAttrs("components.navItem.radius", "colors.navSelected", "colors.navHover", "colors.foreground", "colors.mutedForeground")}
              >
                <Icon style={{ width: v("icon.lg"), height: v("icon.lg") }} />
              </span>
            ))}
          </div>
          <div className="flex flex-col" style={{ gap: v("spacing.xs") }}>
            <button
              type="button"
              onClick={onToggleMode}
              title={t.toggleMode}
              aria-label={t.toggleMode}
              className="grid h-8 w-8 place-items-center hover:bg-[var(--gt-colors-navHover)]"
              style={{ borderRadius: v("components.navItem.radius"), color: v("colors.mutedForeground") }}
              {...tokenAttrs("components.navItem.radius", "colors.navHover", "colors.mutedForeground")}
            >
              <ModeIcon style={{ width: v("icon.lg"), height: v("icon.lg") }} />
            </button>
            <span className="grid h-8 w-8 place-items-center" style={{ color: v("colors.mutedForeground") }}>
              <Settings style={{ width: v("icon.lg"), height: v("icon.lg") }} />
            </span>
          </div>
        </div>

        {/* 侧栏：『状态』文件夹，各状态项内嵌分类子项；无独立『分类』分区 */}
        <div
          className="flex w-[200px] shrink-0 flex-col overflow-y-auto [scrollbar-width:none]"
          style={{ backgroundColor: v("colors.chrome"), borderRight: hairline, padding: v("spacing.sm") }}
          {...tokenAttrs("colors.chrome", "colors.hairline", "spacing.sm")}
        >
          <div
            className="flex items-center justify-between"
            style={{ height: v("density.sectionHeader"), color: v("colors.textTertiary"), paddingInline: v("spacing.sm"), ...text("caption") }}
            {...tokenAttrs("density.sectionHeader", "colors.textTertiary", "typography.caption.size")}
          >
            <span>{t.sectionStatus}</span>
            <ChevronDown style={iconSm} />
          </div>
          {FOLDERS.map((key) => {
            const Icon = FOLDER_ICON[key];
            const count = TASKS.filter(FOLDER_MATCH[key]).length;
            const active = folder === key && category === null;
            const open = expanded === key;
            const iconColor =
              key === "failed" && failedCount > 0 ? v("colors.destructive") : active ? v("colors.foreground") : v("colors.mutedForeground");
            return (
              <div key={key}>
                <button
                  type="button"
                  onClick={() => {
                    setFolder(key);
                    setCategory(null);
                    setExpanded(key);
                  }}
                  className={cn("flex w-full items-center text-left", !active && "hover:bg-[var(--gt-colors-navHover)]")}
                  style={{
                    height: v("density.navRow"),
                    borderRadius: v("components.navItem.radius"),
                    backgroundColor: active ? v("colors.navSelected") : undefined,
                    paddingInline: v("spacing.sm"),
                    gap: v("spacing.sm"),
                    ...text("sm"),
                  }}
                  {...tokenAttrs("density.navRow", "components.navItem.radius", "colors.navSelected", "colors.navHover", "colors.foreground", "colors.mutedForeground")}
                >
                  <ChevronRight
                    className="shrink-0 transition-transform"
                    style={{ ...iconSm, color: v("colors.textTertiary"), transform: open ? "rotate(90deg)" : undefined }}
                  />
                  <Icon className="shrink-0" style={{ ...iconMd, color: iconColor }} />
                  <span className="min-w-0 flex-1 truncate">{t.folders[key]}</span>
                  {key === "downloading" && (
                    <span className="h-1.5 w-1.5 shrink-0 rounded-full" style={{ backgroundColor: v("colors.statusDownloading") }} {...tokenAttrs("colors.statusDownloading")} />
                  )}
                  <span className="tabular-nums" style={{ ...text("caption"), fontFamily: v("typography.mono"), color: v("colors.textTertiary") }}>
                    {count}
                  </span>
                </button>
                {open &&
                  CATEGORIES.map((cat) => {
                    const CatIcon = CATEGORY_ICON[cat];
                    const catCount = TASKS.filter((task) => FOLDER_MATCH[key](task) && task.category === cat).length;
                    const catActive = folder === key && category === cat;
                    return (
                      <button
                        key={cat}
                        type="button"
                        onClick={() => {
                          setFolder(key);
                          setCategory(cat);
                        }}
                        className={cn("flex w-full items-center text-left", !catActive && "hover:bg-[var(--gt-colors-navHover)]")}
                        style={{
                          height: v("density.navRow"),
                          borderRadius: v("components.navItem.radius"),
                          backgroundColor: catActive ? v("colors.navSelected") : undefined,
                          paddingLeft: `calc(${v("spacing.sm")} + ${v("spacing.lg")})`,
                          paddingRight: v("spacing.sm"),
                          gap: v("spacing.sm"),
                          color: catActive ? v("colors.foreground") : v("colors.mutedForeground"),
                          ...text("sm"),
                        }}
                        {...tokenAttrs("density.navRow", "spacing.lg", "components.navItem.radius", "colors.navSelected", "colors.mutedForeground")}
                      >
                        <CatIcon className="shrink-0" style={iconMd} />
                        <span className="min-w-0 flex-1 truncate">{t.categories[cat]}</span>
                        <span className="tabular-nums" style={{ ...text("caption"), fontFamily: v("typography.mono"), color: v("colors.textTertiary") }}>
                          {catCount}
                        </span>
                      </button>
                    );
                  })}
              </div>
            );
          })}

          <div
            className="flex items-center justify-between"
            style={{ height: v("density.sectionHeader"), color: v("colors.textTertiary"), paddingInline: v("spacing.sm"), marginTop: v("spacing.md"), ...text("caption") }}
            {...tokenAttrs("density.sectionHeader", "colors.textTertiary", "spacing.md")}
          >
            <span>{t.sectionQueues}</span>
            <ChevronDown style={iconSm} />
          </div>
          {[t.queueDefault, t.queueNight].map((name, index) => (
            <div
              key={name}
              className="flex items-center hover:bg-[var(--gt-colors-navHover)]"
              style={{ height: v("density.navRow"), borderRadius: v("components.navItem.radius"), paddingInline: v("spacing.sm"), gap: v("spacing.sm"), color: v("colors.mutedForeground"), ...text("sm") }}
              {...tokenAttrs("density.navRow", "components.navItem.radius", "colors.navHover")}
            >
              <Clock className="shrink-0" style={iconMd} />
              <span className="min-w-0 flex-1 truncate">{name}</span>
              <span className="tabular-nums" style={{ ...text("caption"), fontFamily: v("typography.mono"), color: v("colors.textTertiary") }}>
                {index === 0 ? 5 : 1}
              </span>
            </div>
          ))}
        </div>

        {/* 内容区：无网格线任务表 + 浮动选择条 */}
        <div className="relative flex min-w-0 flex-1 flex-col" style={{ backgroundColor: v("colors.background") }} {...tokenAttrs("colors.background")}>
          <div
            className="flex shrink-0 items-center"
            style={{ height: 28, color: v("colors.textTertiary"), paddingInline: v("spacing.xs"), ...text("caption") }}
            {...tokenAttrs("colors.textTertiary", "typography.caption.size", "colors.hairline")}
          >
            <span className="w-9 shrink-0" />
            {[
              { label: t.colName, className: "min-w-0 flex-[2.4]" },
              { label: t.colSize, className: "w-[76px] shrink-0 hidden @xl:flex" },
              { label: t.colProgress, className: "min-w-[90px] flex-[1.4]" },
              { label: t.colStatus, className: "w-[108px] shrink-0" },
              { label: t.colCreated, className: "w-[124px] shrink-0 hidden @3xl:flex" },
            ].map((column) => (
              <span key={column.label} className={cn("flex items-center", column.className)} style={{ gap: v("spacing.sm"), paddingInline: v("spacing.sm") }}>
                <span className="truncate">{column.label}</span>
                <span className="ml-auto h-3.5 shrink-0" style={{ width: v("stroke.thin"), backgroundColor: v("colors.hairline") }} />
              </span>
            ))}
          </div>

          <div className="min-h-0 flex-1 overflow-y-auto [scrollbar-width:thin]" style={{ paddingInline: v("spacing.xs") }}>
            {visible.length === 0 && (
              <p className="py-10 text-center" style={{ color: v("colors.textTertiary") }}>
                {t.noTasks}
              </p>
            )}
            {visible.map((task) => {
              const isSelected = selected.has(task.id);
              const Icon = CATEGORY_ICON[task.category];
              return (
                <div
                  key={task.id}
                  onClick={() => toggleSelected(task.id)}
                  className="group/row relative flex cursor-default items-center"
                  style={{ height: v("density.taskRow") }}
                  {...tokenAttrs("density.taskRow", "components.taskRow.radius", "colors.rowHover", "colors.accent")}
                >
                  <span
                    className={cn("absolute inset-y-0 left-1 right-1", !isSelected && "group-hover/row:bg-[var(--gt-colors-rowHover)]")}
                    style={{ borderRadius: v("components.taskRow.radius"), backgroundColor: isSelected ? v("colors.accent") : undefined }}
                  />
                  <span className="relative grid w-9 shrink-0 place-items-center" style={{ color: v("colors.mutedForeground") }}>
                    {isSelected ? (
                      <CheckMark checked />
                    ) : (
                      <>
                        <Icon className="group-hover/row:hidden" style={{ width: v("icon.lg"), height: v("icon.lg") }} />
                        <span className="hidden group-hover/row:block">
                          <CheckMark checked={false} />
                        </span>
                      </>
                    )}
                  </span>
                  <span className="relative flex min-w-0 flex-[2.4] flex-col justify-center" style={{ paddingInline: v("spacing.sm") }}>
                    <span className="truncate" style={text("sm")} {...tokenAttrs("typography.sm.size", "colors.foreground")}>
                      {task.name}
                    </span>
                    <span className="truncate" style={{ ...text("caption"), color: v("colors.textTertiary") }} {...tokenAttrs("typography.caption.size", "colors.textTertiary")}>
                      {task.speed ? `${task.speed} · ${Math.round(task.progress * 100)}%` : `${Math.round(task.progress * 100)}%`}
                    </span>
                  </span>
                  <span className="relative hidden w-[76px] shrink-0 tabular-nums @xl:block" style={{ paddingInline: v("spacing.sm"), color: v("colors.mutedForeground"), ...text("sm") }}>
                    {task.size}
                  </span>
                  <span className="relative flex min-w-[90px] flex-[1.4] items-center" style={{ paddingInline: v("spacing.sm") }}>
                    <ProgressBar task={task} />
                  </span>
                  <span
                    className="relative flex w-[108px] shrink-0 items-center truncate"
                    style={{ paddingInline: v("spacing.sm"), gap: v("spacing.xs"), color: v(STATUS_TEXT[task.status]), ...text("sm") }}
                    {...tokenAttrs(STATUS_TEXT[task.status])}
                  >
                    {task.status === "downloading" ? <Play style={iconSm} /> : task.status === "paused" ? <Pause style={iconSm} /> : null}
                    {t.status[task.status]}
                  </span>
                  <span
                    className="relative hidden w-[124px] shrink-0 tabular-nums @3xl:block"
                    style={{ paddingInline: v("spacing.sm"), color: v("colors.textTertiary"), fontFamily: v("typography.mono"), ...text("caption") }}
                    {...tokenAttrs("typography.mono", "colors.textTertiary")}
                  >
                    {task.created}
                  </span>
                </div>
              );
            })}
          </div>

          {selected.size > 0 && (
            <div
              className="absolute left-1/2 flex -translate-x-1/2 items-center"
              style={{
                bottom: v("spacing.lg"),
                height: 36,
                backgroundColor: v("colors.surface"),
                color: v("colors.surfaceForeground"),
                border: hairline,
                borderRadius: v("radius.lg"),
                boxShadow: v("shadow.md"),
                paddingInline: v("spacing.sm"),
                gap: v("spacing.xs"),
                ...text("sm"),
              }}
              {...tokenAttrs("colors.surface", "colors.hairline", "radius.lg", "shadow.md", "spacing.lg")}
            >
              <span className="whitespace-nowrap" style={{ paddingInline: v("spacing.xs") }}>
                {t.selected(selected.size)}
              </span>
              <span style={{ width: v("stroke.thin"), height: v("icon.lg"), backgroundColor: v("colors.hairline") }} />
              {[Play, Pause, Folder].map((Icon, index) => (
                <span key={index} className="grid h-7 w-7 place-items-center hover:bg-[var(--gt-colors-rowHover)]" style={{ borderRadius: v("components.button.radius") }}>
                  <Icon style={iconMd} />
                </span>
              ))}
              <span className="grid h-7 w-7 place-items-center" style={{ color: v("colors.destructive") }} {...tokenAttrs("colors.destructive")}>
                <Trash2 style={iconMd} />
              </span>
              <span style={{ width: v("stroke.thin"), height: v("icon.lg"), backgroundColor: v("colors.hairline") }} />
              <button type="button" onClick={() => setSelected(new Set())} aria-label={t.clearSelection} className="grid h-7 w-7 place-items-center">
                <X style={iconMd} />
              </button>
            </div>
          )}
        </div>
      </div>

      {/* 状态栏 */}
      <div
        className="flex shrink-0 items-center"
        style={{
          height: v("density.statusBar"),
          backgroundColor: v("colors.chrome"),
          borderTop: hairline,
          paddingInline: v("spacing.sm"),
          gap: v("spacing.sm"),
          color: v("colors.mutedForeground"),
          ...text("caption"),
        }}
        {...tokenAttrs("density.statusBar", "colors.chrome", "colors.hairline", "colors.mutedForeground", "typography.caption.size")}
      >
        <span className="inline-flex items-center tabular-nums" style={{ gap: v("spacing.xs"), color: v("colors.statusDownloading"), fontFamily: v("typography.mono") }} {...tokenAttrs("colors.statusDownloading", "typography.mono")}>
          <ArrowDownToLine style={iconSm} />
          12.8 MB/s
        </span>
        <span
          className="inline-flex items-center hover:bg-[var(--gt-colors-navHover)]"
          style={{ height: v("density.statusControl"), paddingInline: v("spacing.xs"), gap: v("spacing.xs"), borderRadius: v("components.button.radius") }}
          {...tokenAttrs("density.statusControl", "spacing.xs", "components.button.radius", "colors.navHover")}
        >
          <Pause style={iconSm} />
          {t.pauseAll}
        </span>
        <span className="ml-auto inline-flex items-center" style={{ gap: v("spacing.xs") }}>
          {[
            { Icon: ArrowDownToLine, label: t.unlimited },
            { Icon: ArrowUp, label: t.unlimited },
            { Icon: HardDrive, label: t.freeSpace("128 GB") },
          ].map(({ Icon, label }, index) => (
            <span
              key={index}
              className="inline-flex items-center tabular-nums hover:bg-[var(--gt-colors-navHover)]"
              style={{ height: v("density.statusControl"), paddingInline: v("spacing.xs"), gap: v("spacing.xs"), borderRadius: v("components.button.radius") }}
              {...tokenAttrs("density.statusControl", "components.button.radius", "colors.navHover")}
            >
              <Icon style={iconSm} />
              {label}
            </span>
          ))}
          <Badge tone="muted">{mode}</Badge>
        </span>
      </div>

      {dialog && <NewDownloadDialog t={t} onClose={() => setDialog(false)} />}
    </div>
  );
}
