import { Fragment } from "react";

import { inlineMarkdown } from "@/lib/documents";

/** Renders **bold** and [links](url) the way the Word file does. */
export function InlineMarkdown({ text }: { text: string }) {
  return (
    <>
      {inlineMarkdown(text).map((s, i) =>
        s.bold ? (
          <strong key={i}>{s.text}</strong>
        ) : s.href ? (
          <a key={i} href={s.href} target="_blank" rel="noopener noreferrer" className="underline-offset-2 hover:underline">
            {s.text}
          </a>
        ) : (
          <Fragment key={i}>{s.text}</Fragment>
        ),
      )}
    </>
  );
}
