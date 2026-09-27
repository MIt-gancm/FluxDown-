/** 反馈页岛:反馈追踪看板 / 提交反馈 两个标签 + 共享的 Issue 详情对话框。 */
import { useEffect, useState } from "react";
import { LayoutDashboard, Plus } from "lucide-react";
import type { Lang } from "@/i18n/config";
import { feedback } from "@/i18n/messages/feedback";
import ProjectBoard from "./board/ProjectBoard";
import FeedbackForm from "./FeedbackForm";
import IssueDialog from "./IssueDialog";
import { Notice, Tabs } from "./shared";

type Tab = "board" | "submit";

export default function FeedbackApp({ lang }: { lang: Lang }) {
  const t = feedback[lang];
  const [tab, setTab] = useState<Tab>("board");
  const [issue, setIssue] = useState<number | null>(null);
  const [submitted, setSubmitted] = useState(false);

  useEffect(() => {
    if (!submitted) return;
    const id = window.setTimeout(() => setSubmitted(false), 5000);
    return () => window.clearTimeout(id);
  }, [submitted]);

  return (
    <>
      <div className="flex flex-wrap items-center gap-4 border-b border-line px-[clamp(20px,4vw,56px)] py-5">
        <Tabs
          label={t.tabs.label}
          idPrefix="feedback"
          active={tab}
          onChange={setTab}
          tabs={[
            { key: "board", label: t.tabs.board, icon: LayoutDashboard },
            { key: "submit", label: t.tabs.submit, icon: Plus },
          ]}
        />
        {submitted && <Notice tone="ok">{t.form.success}</Notice>}
      </div>

      <div role="tabpanel" id={`feedback-panel-${tab}`} aria-labelledby={`feedback-tab-${tab}`}>
        {tab === "board" ? (
          <ProjectBoard lang={lang} onOpen={setIssue} />
        ) : (
          <div className="section-tight">
            <FeedbackForm
              lang={lang}
              onSuccess={() => {
                setSubmitted(true);
                setTab("board");
              }}
            />
          </div>
        )}
      </div>

      <IssueDialog lang={lang} issueNumber={issue} onClose={() => setIssue(null)} />
    </>
  );
}
