import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { ApplicationQuestion } from "@/lib/api";

import { QuestionsForm } from "./questions-form";

afterEach(cleanup);

const questions: ApplicationQuestion[] = [
  { id: "q1", label: "Do you hold a Working With Children Check?", kind: "radio", options: ["Yes", "No"], answer: null, answered_at: null, savable: true },
  { id: "q2", label: "Why do you want this role?", kind: "textarea", options: [], answer: null, answered_at: null, savable: true },
];

describe("QuestionsForm", () => {
  it("needs every answer, and saves short answers to the bank by default", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(<QuestionsForm questions={questions} onSubmit={onSubmit} />);

    const send = screen.getByRole("button", { name: /send answers/i });
    expect(send).toBeDisabled();

    fireEvent.change(screen.getByLabelText(/Working With Children/), { target: { value: "No" } });
    expect(send).toBeDisabled();
    fireEvent.change(screen.getByLabelText(/Why do you want/), { target: { value: "I like data." } });
    expect(send).toBeEnabled();

    const saves = screen.getAllByLabelText(/save to my answer bank/i);
    expect(saves[0]).toBeChecked();
    expect(saves[1]).not.toBeChecked();

    fireEvent.click(send);
    await vi.waitFor(() => expect(onSubmit).toHaveBeenCalled());
    expect(onSubmit).toHaveBeenCalledWith([
      { question_id: "q1", answer: "No", save: true },
      { question_id: "q2", answer: "I like data.", save: false },
    ]);
  });

  it("never offers to save the agent's own questions", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const cover: ApplicationQuestion = {
      id: "q3",
      label: "This job accepts a cover letter. Include one?",
      kind: "radio",
      options: ["No, skip it", "Yes, I'll add one"],
      answer: null,
      answered_at: null,
      savable: false,
    };
    render(<QuestionsForm questions={[cover]} onSubmit={onSubmit} />);
    expect(screen.queryByLabelText(/save to my answer bank/i)).toBeNull();
    expect(screen.getByText(/only for this application/i)).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText(/cover letter/), { target: { value: "No, skip it" } });
    fireEvent.click(screen.getByRole("button", { name: /send answers/i }));
    await vi.waitFor(() => expect(onSubmit).toHaveBeenCalled());
    expect(onSubmit).toHaveBeenCalledWith([{ question_id: "q3", answer: "No, skip it", save: false }]);
  });
});
