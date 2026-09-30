/**
 * 全站导航模型:Header 下拉、移动端菜单、Footer、命令面板共用这一份结构。
 * `label` / `desc` 是 `common.nav` 的键;路径为无语言前缀的规范路径,渲染时经 `href()` 本地化。
 */
import {
  Braces,
  Heart,
  History,
  Megaphone,
  MessageSquare,
  Palette,
  PenTool,
  Puzzle,
  Send,
  Sparkles,
  Users,
  Vote,
  type LucideIcon,
} from "lucide-react";
import type { common } from "@/i18n/messages/common";
import type { Lang } from "@/i18n/config";

type NavKey = keyof (typeof common)["en"]["nav"];

export interface NavLink {
  label: NavKey;
  desc?: NavKey;
  path: string;
  icon: LucideIcon;
}

export const GITHUB_URL = "https://github.com/zerx-lab/FluxDown";
export const TELEGRAM_URL = "https://t.me/+tp8ie_FVjv02ZDNl";

export const PRODUCT_LINKS: NavLink[] = [
  { label: "features", desc: "featuresDesc", path: "/#features", icon: Sparkles },
  { label: "plugins", desc: "pluginsDesc", path: "/plugins", icon: Puzzle },
  { label: "themes", desc: "themesDesc", path: "/themes", icon: Palette },
  { label: "themeBuilder", desc: "themeBuilderDesc", path: "/theme-builder", icon: PenTool },
  { label: "apiDocs", desc: "apiDocsDesc", path: "/api-docs", icon: Braces },
  { label: "changelog", desc: "changelogDesc", path: "/changelog", icon: History },
];

export const COMMUNITY_LINKS: NavLink[] = [
  { label: "announcements", desc: "announcementsDesc", path: "/announcements", icon: Megaphone },
  { label: "feedback", desc: "feedbackDesc", path: "/feedback", icon: MessageSquare },
  { label: "featureVote", desc: "featureVoteDesc", path: "/feature-vote", icon: Vote },
  { label: "telegram", desc: "telegramDesc", path: "/telegram-group", icon: Send },
  { label: "qq", desc: "qqDesc", path: "/qq-group", icon: Users },
];

/** 赞助是一级导航(不进社区下拉),首页打开即可见。 */
export const SPONSOR_LINK: NavLink = { label: "sponsor", path: "/sponsor", icon: Heart };

/** 文档入口直接落到对应语言,避免 `/docs/` 的 302。 */
export const docsPath = (lang: Lang) => `/docs/${lang}/`;
