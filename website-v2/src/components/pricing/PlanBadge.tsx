import { BadgeCheck } from "lucide-react";
import type { CloudPlan } from "./types";

/**
 * Operator-configured plan badge. Colors are arbitrary hex values from the catalog, so they must
 * be inline styles. `ribbon` renders as a corner tag pinned to the (relative) plan cell; the
 * other styles render inline next to the plan name. The site has no signed-in context, so
 * `badgeNumbered` never shows a personal number — only the label.
 */
export function PlanBadge({ plan }: { plan: CloudPlan }) {
  if (!plan.badge || plan.badgeStyle === "ribbon") return null;
  const color = plan.badgeColor;

  if (plan.badgeStyle === "solid") {
    return (
      <span
        className="inline-flex h-[22px] items-center gap-1 rounded-full px-2 text-[11px] font-semibold text-white"
        style={{ backgroundColor: color }}
      >
        <BadgeCheck size={12} aria-hidden />
        {plan.badge}
      </span>
    );
  }

  if (plan.badgeStyle === "medal") {
    return (
      <span
        className="inline-flex h-[22px] items-stretch overflow-hidden rounded-full border text-[11px] font-semibold"
        style={{ borderColor: color }}
      >
        <span className="flex items-center px-1.5 text-white" style={{ backgroundColor: color }}>
          <BadgeCheck size={12} aria-hidden />
        </span>
        <span className="flex items-center px-2" style={{ color, backgroundColor: `${color}1a` }}>
          {plan.badge}
        </span>
      </span>
    );
  }

  return (
    <span
      className="inline-flex h-[22px] items-center gap-1 rounded-full border px-2 text-[11px] font-semibold"
      style={{ borderColor: color, color, backgroundColor: `${color}1a` }}
    >
      <BadgeCheck size={12} aria-hidden />
      {plan.badge}
    </span>
  );
}

/** `ribbon` style: a square corner tag flush with the cell's top-right edge. */
export function PlanRibbon({ plan }: { plan: CloudPlan }) {
  if (!plan.badge || plan.badgeStyle !== "ribbon") return null;
  return (
    <span
      className="absolute right-0 top-0 z-[1] px-2.5 py-1 font-mono text-[10.5px] font-semibold uppercase tracking-[0.1em] text-white"
      style={{ backgroundColor: plan.badgeColor }}
    >
      {plan.badge}
    </span>
  );
}
