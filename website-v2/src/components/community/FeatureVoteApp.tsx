/** 路线图页岛:路线图 / 功能投票 两个标签 + 共享的 Issue 详情对话框。 */
import { useState } from "react";
import { Map as MapIcon, ThumbsUp } from "lucide-react";
import type { Lang } from "@/i18n/config";
import { roadmap } from "@/i18n/messages/roadmap";
import FeatureVote from "./FeatureVote";
import IssueDialog from "./IssueDialog";
import Roadmap from "./Roadmap";
import { Tabs } from "./shared";

type Tab = "roadmap" | "vote";

export default function FeatureVoteApp({ lang }: { lang: Lang }) {
  const t = roadmap[lang];
  const [tab, setTab] = useState<Tab>("roadmap");
  const [issue, setIssue] = useState<number | null>(null);
  const panel = tab === "roadmap" ? t.board : t.vote;

  return (
    <>
      <div className="flex flex-wrap items-center gap-4 border-b border-line px-[clamp(20px,4vw,56px)] py-5">
        <Tabs
          label={t.tabs.label}
          idPrefix="roadmap"
          active={tab}
          onChange={setTab}
          tabs={[
            { key: "roadmap", label: t.tabs.roadmap, icon: MapIcon },
            { key: "vote", label: t.tabs.vote, icon: ThumbsUp },
          ]}
        />
      </div>

      <div role="tabpanel" id={`roadmap-panel-${tab}`} aria-labelledby={`roadmap-tab-${tab}`}>
        <div className="flex flex-col gap-2 px-[clamp(20px,4vw,56px)] pt-8 pb-6">
          <h2 className="h3">{panel.heading}</h2>
          <p className="max-w-[62ch] text-sm leading-relaxed text-muted">{panel.subtitle}</p>
        </div>
        {tab === "roadmap" ? (
          <div className="border-t border-line">
            <Roadmap lang={lang} onOpen={setIssue} />
          </div>
        ) : (
          <div className="mx-auto w-full max-w-3xl px-[clamp(20px,4vw,56px)] pb-[clamp(40px,6vw,72px)]">
            <FeatureVote lang={lang} onOpen={setIssue} />
          </div>
        )}
      </div>

      <IssueDialog lang={lang} issueNumber={issue} onClose={() => setIssue(null)} />
    </>
  );
}
