import { useState, useCallback } from "react";
import { motion } from "framer-motion";
import { Plus, List } from "lucide-react";
import { useLocale } from "@/lib/i18n";
import FeedbackSection from "./FeedbackSection";
import FeedbackListSection from "./FeedbackListSection";
import IssueDetailModal from "./IssueDetailModal";

type TabKey = "list" | "submit";

export default function FeedbackPage() {
  const { t } = useLocale();
  const [activeTab, setActiveTab] = useState<TabKey>("list");
  const [selectedIssue, setSelectedIssue] = useState<number | null>(null);

  const handleIssueClick = useCallback((issueNumber: number) => {
    setSelectedIssue(issueNumber);
  }, []);

  const handleCloseDetail = useCallback(() => {
    setSelectedIssue(null);
  }, []);

  const tabs: { key: TabKey; icon: typeof List; labelKey: string }[] = [
    { key: "list", icon: List, labelKey: "fbPage.tabList" },
    { key: "submit", icon: Plus, labelKey: "fbPage.tabSubmit" },
  ];

  return (
    <>
      {/* Tab Switcher */}
      <div className="pt-24 pb-0 bg-dark-bg">
        <div className="mx-auto max-w-4xl px-4 sm:px-6 lg:px-8">
          <div className="flex items-center justify-center">
            <div className="inline-flex items-center gap-1 p-1 rounded-lg bg-dark-surface1 border border-dark-border">
              {tabs.map(({ key, icon: Icon, labelKey }) => (
                <button
                  key={key}
                  onClick={() => setActiveTab(key)}
                  className={`relative flex items-center gap-1.5 px-4 py-2 rounded-md text-sm font-medium transition-all duration-200 cursor-pointer ${
                    activeTab === key
                      ? "text-dark-text"
                      : "text-dark-text-secondary hover:text-dark-text-muted"
                  }`}
                >
                  <Icon className="w-4 h-4" />
                  {t(labelKey)}
                  {activeTab === key && (
                    <motion.div
                      layoutId="feedback-tab-bg"
                      className="absolute inset-0 rounded-md bg-dark-surface2 border border-dark-border -z-10"
                      transition={{ type: "spring", bounce: 0.15, duration: 0.4 }}
                    />
                  )}
                </button>
              ))}
            </div>
          </div>
        </div>
      </div>

      {/* Tab Content */}
      {activeTab === "list" ? (
        <FeedbackListSection onIssueClick={handleIssueClick} />
      ) : (
        <FeedbackSection />
      )}

      {/* Issue Detail Modal */}
      <IssueDetailModal
        issueNumber={selectedIssue}
        onClose={handleCloseDetail}
      />
    </>
  );
}
