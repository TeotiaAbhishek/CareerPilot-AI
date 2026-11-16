import { Search, TriangleAlert } from "lucide-react";
import Link from "next/link";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

import type { Chip } from "@/lib/api";
import { linkKind } from "@/lib/assistant";

/** What the assistant looked up, as small chips. */
export function Chips({ chips }: { chips: Chip[] }) {
  if (chips.length === 0) return null;
  return (
    <ul className="mb-2 flex flex-wrap gap-1.5" aria-label="What the assistant looked at">
      {chips.map((chip, i) => (
        <li
          key={i}
          className="inline-flex items-center gap-1 rounded-full border bg-muted px-2 py-0.5 text-xs text-muted-foreground"
        >
          {chip.label.startsWith("Couldn't") ? (
            <TriangleAlert className="size-3 text-warning" aria-hidden />
          ) : (
            <Search className="size-3" aria-hidden />
          )}
          {chip.label}
        </li>
      ))}
    </ul>
  );
}

/** An answer in Markdown. Job links open in the app; other https links in a new tab. */
export function AnswerText({ text }: { text: string }) {
  return (
    <div className="prose prose-sm max-w-none break-words dark:prose-invert prose-p:my-2 prose-ul:my-2 prose-li:my-0.5">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          a: ({ href, children }) => {
            const kind = linkKind(href);
            if (kind === "internal") return <Link href={href!}>{children}</Link>;
            if (kind === "external")
              return (
                <a href={href} target="_blank" rel="noreferrer">
                  {children}
                </a>
              );
            return <span>{children}</span>;
          },
          img: () => null,
        }}
      >
        {text}
      </ReactMarkdown>
    </div>
  );
}
