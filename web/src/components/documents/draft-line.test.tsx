import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { Flag } from "@/lib/api";

import { DraftLineEditor, type LineContext } from "./draft-line";
import { groupByJob } from "./documents-list";

afterEach(cleanup);

const flag: Flag = {
  key: "term|summary|AWS",
  path: "summary",
  severity: "error",
  message: "\"AWS\" isn't anywhere in your resume.",
};

function ctx(dismissed: string[], onDismiss = vi.fn()): LineContext {
  return { flags: [flag], dismissed, evidence: new Map([["W1.1", "Built a fraud model."]]), onDismiss };
}

describe("DraftLineEditor", () => {
  it("shows the evidence and lets you accept a flag", () => {
    const onDismiss = vi.fn();
    const onChange = vi.fn();
    render(
      <DraftLineEditor
        label="Summary"
        path="summary"
        line={{ text: "Built AWS fraud models.", evidence: ["W1.1", "W9"] }}
        ctx={ctx([], onDismiss)}
        onChange={onChange}
      />,
    );

    expect(screen.getByLabelText("Evidence W1.1: Built a fraud model.")).toBeTruthy();
    expect(screen.getByLabelText("Evidence W9 (not in your resume)")).toBeTruthy();
    expect(screen.getByRole("textbox", { name: "Summary" }).getAttribute("aria-invalid")).toBe("true");

    fireEvent.change(screen.getByRole("textbox", { name: "Summary" }), { target: { value: "Built fraud models." } });
    expect(onChange).toHaveBeenCalledWith("Built fraud models.");

    fireEvent.click(screen.getByRole("button", { name: /It's true, keep it/ }));
    expect(onDismiss).toHaveBeenCalledWith(flag.key, true);
  });

  it("shows a dismissed flag with an undo", () => {
    const onDismiss = vi.fn();
    render(
      <DraftLineEditor
        label="Summary"
        path="summary"
        line={{ text: "x", evidence: [] }}
        ctx={ctx([flag.key], onDismiss)}
        onChange={() => {}}
      />,
    );
    expect(screen.getByRole("textbox", { name: "Summary" }).getAttribute("aria-invalid")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: /Undo/ }));
    expect(onDismiss).toHaveBeenCalledWith(flag.key, false);
  });
});

describe("groupByJob", () => {
  it("puts a job's documents together, newest first", () => {
    const row = (job_id: string, kind: "resume" | "cover_letter", updated_at: string) => ({
      id: `${job_id}-${kind}`,
      job_id,
      job_title: job_id,
      company: null,
      kind,
      status: "ready" as const,
      open_errors: 0,
      created_at: updated_at,
      updated_at,
      approved_at: null,
    });
    const groups = groupByJob([
      row("a", "resume", "2026-11-01T00:00:00Z"),
      row("b", "resume", "2026-11-02T00:00:00Z"),
      row("a", "cover_letter", "2026-11-03T00:00:00Z"),
    ]);
    expect(groups.map((g) => g.job_id)).toEqual(["a", "b"]);
    expect(Object.keys(groups[0].docs).sort()).toEqual(["cover_letter", "resume"]);
  });
});
