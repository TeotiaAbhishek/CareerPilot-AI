"use client";

import { Printer } from "lucide-react";
import { useSearchParams } from "next/navigation";
import { Fragment } from "react";

import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { useApi } from "@/hooks/use-api";
import type { DraftEntry, JobDocuments, LetterDraft, ResumeDraft } from "@/lib/api";

import { InlineMarkdown as Inline } from "./inline-markdown";

function Heading({ children }: { children: string }) {
  return (
    <h2 className="mt-4 mb-1.5 border-b border-[#1f3a5f]/30 pb-0.5 text-[11pt] font-bold tracking-wide text-[#1f3a5f] uppercase">
      {children}
    </h2>
  );
}

function Entries({ title, entries }: { title: string; entries: DraftEntry[] }) {
  if (!entries.length) return null;
  return (
    <>
      <Heading>{title}</Heading>
      {entries.map((e) => (
        <div key={e.id} className="mb-2 break-inside-avoid">
          <p className="font-bold">{e.heading}</p>
          {e.details.length ? (
            <p>
              <Inline text={e.details.join(" | ")} />
            </p>
          ) : null}
          {e.bullets.length ? (
            <ul className="mt-0.5 list-disc pl-5">
              {e.bullets.map((b, i) => (
                <li key={i}>{b.text}</li>
              ))}
            </ul>
          ) : null}
        </div>
      ))}
    </>
  );
}

function ResumeSheet({ draft }: { draft: ResumeDraft }) {
  return (
    <>
      <h1 className="text-[18pt] font-bold">{draft.name}</h1>
      <p className="text-[9.5pt]">
        <Inline text={draft.contact.join(" | ")} />
      </p>
      {draft.summary.text ? (
        <>
          <Heading>Professional Summary</Heading>
          <p>{draft.summary.text}</p>
        </>
      ) : null}
      {draft.skills.length ? (
        <>
          <Heading>Skills</Heading>
          {draft.skills.map((g, i) => (
            <p key={i}>
              {g.group ? <strong>{g.group}: </strong> : null}
              {g.items.join(", ")}
            </p>
          ))}
        </>
      ) : null}
      <Entries title="Experience" entries={draft.experience} />
      <Entries title="Projects" entries={draft.projects} />
      <Entries title="Education" entries={draft.education} />
      {draft.other.map((s) => (
        <Fragment key={s.heading}>
          <Heading>{s.heading}</Heading>
          <ul className="list-disc pl-5">
            {s.lines.map(([id, text]) => (
              <li key={id || text}>
                <Inline text={text} />
              </li>
            ))}
          </ul>
        </Fragment>
      ))}
    </>
  );
}

function LetterSheet({ draft }: { draft: LetterDraft }) {
  const today = new Date().toLocaleDateString("en-AU", { day: "numeric", month: "long", year: "numeric" });
  return (
    <div className="flex flex-col gap-3 text-[11pt]">
      <div>
        <h1 className="text-[18pt] font-bold">{draft.name}</h1>
        <p className="text-[9.5pt]">
          <Inline text={draft.contact.join(" | ")} />
        </p>
      </div>
      <p>{today}</p>
      <p>{draft.greeting}</p>
      {draft.paragraphs.map((p, i) => (
        <p key={i}>{p.text}</p>
      ))}
      <p>
        {draft.closing}
        <br />
        {draft.name}
      </p>
    </div>
  );
}

/** A print-ready page (A4, black on white). Print it and choose "Save as PDF". */
export function DocumentPreview({ jobId }: { jobId: string }) {
  const kind = useSearchParams().get("tab") === "cover_letter" ? "cover_letter" : "resume";
  const { data, error } = useApi<JobDocuments>(`/api/jobs/${encodeURIComponent(jobId)}/documents`);
  const document = data?.[kind];

  if (error) return <p className="text-sm text-danger">{error}</p>;
  if (!data) return <Skeleton className="mx-auto h-[29.7cm] w-full max-w-[21cm]" />;
  if (!document?.content) {
    return <p className="text-sm text-muted-foreground">This document isn&apos;t ready yet.</p>;
  }

  return (
    <>
      <div className="mb-4 print:hidden flex flex-wrap items-center justify-between gap-3">
        <p className="text-sm text-muted-foreground">
          {document.status === "approved" ? "Approved." : "Not approved yet."} In the print dialog choose
          &quot;Save as PDF&quot;, A4, and turn off headers and footers.
        </p>
        <Button onClick={() => window.print()}>
          <Printer /> Print / Save as PDF
        </Button>
      </div>
      <article className="print-sheet mx-auto w-full max-w-[21cm] bg-white px-[1.5cm] py-[1.25cm] font-[Calibri,Carlito,Arial,sans-serif] text-[10.5pt] leading-snug text-black shadow-lg print:max-w-none print:shadow-none">
        {document.kind === "resume" ? (
          <ResumeSheet draft={document.content} />
        ) : (
          <LetterSheet draft={document.content} />
        )}
      </article>
    </>
  );
}
