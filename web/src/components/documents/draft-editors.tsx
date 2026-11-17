"use client";

import { Trash2, X } from "lucide-react";
import { Fragment, type ReactNode } from "react";

import { Button } from "@/components/ui/button";
import type { DraftEntry, DraftLine, LetterDraft, ResumeDraft } from "@/lib/api";
import { flagsAt, move, wordCount } from "@/lib/documents";

import { DraftLineEditor, type LineContext } from "./draft-line";
import { InlineMarkdown } from "./inline-markdown";

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-3">
      <h3 className="text-xs font-semibold tracking-wider text-muted-foreground uppercase">{title}</h3>
      {children}
    </section>
  );
}

/** Lines the model can't change (names, titles, dates), shown read-only. */
function Fixed({ children }: { children: ReactNode }) {
  return <p className="text-sm text-muted-foreground">{children}</p>;
}

const plain = (text: string) => text.replaceAll("**", "");

export function ResumeDraftEditor({
  draft,
  ctx,
  onChange,
}: {
  draft: ResumeDraft;
  ctx: LineContext;
  onChange: (draft: ResumeDraft) => void;
}) {
  const setEntries = (section: "experience" | "projects", entries: DraftEntry[]) =>
    onChange({ ...draft, [section]: entries });

  const entryEditor = (section: "experience" | "projects", entry: DraftEntry, index: number, all: DraftEntry[]) => {
    const setBullets = (bullets: DraftLine[]) =>
      setEntries(section, all.map((e, i) => (i === index ? { ...e, bullets } : e)));
    return (
      <div key={entry.id} className="flex flex-col gap-3 rounded-lg border p-4">
        <div className="flex items-start justify-between gap-3">
          <div className="min-w-0">
            <p className="font-medium">{entry.heading}</p>
            {entry.details.length ? <Fixed>{entry.details.map(plain).join(" · ")}</Fixed> : null}
          </div>
          {section === "projects" ? (
            <Button
              variant="ghost"
              size="sm"
              className="shrink-0 text-muted-foreground"
              disabled={ctx.disabled || all.length === 1}
              onClick={() => setEntries(section, all.filter((_, i) => i !== index))}
            >
              <Trash2 /> Remove project
            </Button>
          ) : null}
        </div>
        {entry.bullets.map((bullet, b) => (
          <Fragment key={`${entry.id}-${b}`}>
            {bullet.group && bullet.group !== entry.bullets[b - 1]?.group ? (
              <p className="-mb-1 text-sm font-medium text-muted-foreground">{bullet.group}</p>
            ) : null}
            <DraftLineEditor
              label={`${entry.heading} bullet ${b + 1}`}
              line={bullet}
              path={`${section}.${entry.id}.${b}`}
              ctx={ctx}
              onChange={(text) => setBullets(entry.bullets.map((l, i) => (i === b ? { ...l, text } : l)))}
              onRemove={entry.bullets.length > 1 ? () => setBullets(entry.bullets.filter((_, i) => i !== b)) : undefined}
              onMove={(d) => setBullets(move(entry.bullets, b, b + d))}
            />
          </Fragment>
        ))}
      </div>
    );
  };

  const documentFlags = flagsAt(ctx.flags, "document", ctx.dismissed).open;

  return (
    <div className="flex flex-col gap-8">
      {documentFlags.length ? (
        <ul className="flex flex-col gap-1.5 rounded-lg bg-muted p-3 text-sm text-muted-foreground">
          {documentFlags.map((f) => (
            <li key={f.key}>{f.message}</li>
          ))}
        </ul>
      ) : null}

      <div>
        <p className="text-lg font-semibold">{draft.name}</p>
        <Fixed>
          <InlineMarkdown text={draft.contact.join(" · ")} />
        </Fixed>
      </div>

      <Section title="Professional summary">
        <DraftLineEditor
          label="Summary"
          line={draft.summary}
          path="summary"
          ctx={ctx}
          onChange={(text) => onChange({ ...draft, summary: { ...draft.summary, text } })}
        />
      </Section>

      <Section title="Skills">
        <div className="flex flex-col gap-2">
          {draft.skills.map((group, g) => (
            <div key={`${group.group}-${g}`} className="flex flex-wrap items-center gap-1.5 text-sm">
              {group.group ? <span className="mr-1 font-medium">{group.group}:</span> : null}
              {group.items.map((item) => (
                <span key={item} className="inline-flex items-center gap-1 rounded-full border bg-card py-0.5 pr-1 pl-2.5 text-xs">
                  {item}
                  <button
                    type="button"
                    aria-label={`Remove ${item}`}
                    disabled={ctx.disabled}
                    className="rounded-full p-0.5 text-muted-foreground hover:bg-muted hover:text-foreground"
                    onClick={() =>
                      onChange({
                        ...draft,
                        skills: draft.skills
                          .map((s, i) => (i === g ? { ...s, items: s.items.filter((x) => x !== item) } : s))
                          .filter((s) => s.items.length > 0),
                      })
                    }
                  >
                    <X className="size-3" />
                  </button>
                </span>
              ))}
            </div>
          ))}
          <p className="text-xs text-muted-foreground">Only skills from your master resume can appear here.</p>
        </div>
      </Section>

      <Section title="Experience">
        {draft.experience.map((e, i, all) => entryEditor("experience", e, i, all))}
      </Section>

      {draft.projects.length ? (
        <Section title="Projects">{draft.projects.map((e, i, all) => entryEditor("projects", e, i, all))}</Section>
      ) : null}

      {draft.education.length ? (
        <Section title="Education">
          {draft.education.map((e) => (
            <div key={e.id}>
              <p className="text-sm font-medium">{e.heading}</p>
              <Fixed>{e.details.map(plain).join(" · ")}</Fixed>
            </div>
          ))}
        </Section>
      ) : null}

      {draft.other.map((section) => (
        <Section key={section.heading} title={section.heading}>
          <ul className="list-disc pl-5 text-sm text-muted-foreground">
            {section.lines.map(([id, text]) => (
              <li key={id || text}>{plain(text)}</li>
            ))}
          </ul>
        </Section>
      ))}
      <p className="text-xs text-muted-foreground">
        Names, employers, titles, dates, education and certifications always come straight from your master
        resume. Edit them on the Profile page.
      </p>
    </div>
  );
}

export function LetterDraftEditor({
  draft,
  ctx,
  onChange,
}: {
  draft: LetterDraft;
  ctx: LineContext;
  onChange: (draft: LetterDraft) => void;
}) {
  const words = wordCount(draft.paragraphs);
  const setParagraphs = (paragraphs: DraftLine[]) => onChange({ ...draft, paragraphs });
  return (
    <div className="flex flex-col gap-5">
      <p className={words > 350 ? "text-sm text-warning" : "text-sm text-muted-foreground"}>
        {words} words {words > 350 ? "(aim for 350 or fewer)" : "· 250-350 is ideal"}
      </p>
      <p className="text-sm">{draft.greeting}</p>
      {draft.paragraphs.map((p, i) => (
        <DraftLineEditor
          key={i}
          label={`Paragraph ${i + 1}`}
          line={p}
          path={`paragraphs.${i}`}
          ctx={ctx}
          onChange={(text) => setParagraphs(draft.paragraphs.map((l, j) => (j === i ? { ...l, text } : l)))}
          onRemove={draft.paragraphs.length > 1 ? () => setParagraphs(draft.paragraphs.filter((_, j) => j !== i)) : undefined}
          onMove={(d) => setParagraphs(move(draft.paragraphs, i, i + d))}
        />
      ))}
      <p className="text-sm">
        {draft.closing}
        <br />
        {draft.name}
      </p>
    </div>
  );
}
