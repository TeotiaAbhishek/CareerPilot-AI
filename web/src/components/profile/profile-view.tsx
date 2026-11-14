"use client";

import { FileText, KeyRound, ListChecks, MessagesSquare, type LucideIcon } from "lucide-react";
import { usePathname, useRouter, useSearchParams } from "next/navigation";

import { PageHeader } from "@/components/ui/page-header";
import { cn } from "@/lib/utils";

import { AnswersManager } from "./answers-manager";
import { FactsForm } from "./facts-form";
import { ResumeEditor } from "./resume-editor";
import { VaultManager } from "./vault-manager";

type Tab = "resume" | "facts" | "answers" | "vault";

const TABS: { value: Tab; label: string; icon: LucideIcon }[] = [
  { value: "resume", label: "Resume", icon: FileText },
  { value: "facts", label: "Facts", icon: ListChecks },
  { value: "answers", label: "Answers", icon: MessagesSquare },
  { value: "vault", label: "Vault", icon: KeyRound },
];

/** Everything the app knows about you, in four tabs. The tab is in the URL (?tab=vault). */
export function ProfileView() {
  const router = useRouter();
  const pathname = usePathname();
  const params = useSearchParams();
  const requested = params.get("tab") as Tab | null;
  const tab: Tab = requested && TABS.some((t) => t.value === requested) ? requested : "resume";

  return (
    <>
      <PageHeader
        title="Profile"
        description="Your master resume, the facts forms ask for, your saved answers and logins."
      />

      <div role="tablist" aria-label="Profile sections" className="mb-6 flex gap-1 overflow-x-auto border-b">
        {TABS.map((t) => (
          <button
            key={t.value}
            type="button"
            role="tab"
            id={`tab-${t.value}`}
            aria-selected={tab === t.value}
            aria-controls={`panel-${t.value}`}
            onClick={() => router.replace(t.value === "resume" ? pathname : `${pathname}?tab=${t.value}`, { scroll: false })}
            className={cn(
              "-mb-px flex items-center gap-2 whitespace-nowrap border-b-2 px-3 py-2 text-sm font-medium transition-colors",
              tab === t.value
                ? "border-primary text-foreground"
                : "border-transparent text-muted-foreground hover:text-foreground",
            )}
          >
            <t.icon className="size-4" aria-hidden />
            {t.label}
          </button>
        ))}
      </div>

      <div role="tabpanel" id={`panel-${tab}`} aria-labelledby={`tab-${tab}`}>
        {tab === "resume" ? <ResumeEditor /> : null}
        {tab === "facts" ? <FactsForm /> : null}
        {tab === "answers" ? <AnswersManager /> : null}
        {tab === "vault" ? <VaultManager /> : null}
      </div>
    </>
  );
}
