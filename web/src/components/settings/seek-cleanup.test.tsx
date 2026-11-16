import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({
  scanSeekResumes: vi.fn(),
  deleteSeekResume: vi.fn(),
}));

vi.mock("@/lib/api", async (original) => ({ ...(await original<typeof import("@/lib/api")>()), ...api }));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() } }));

import { SeekCleanup } from "./seek-cleanup";

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("SeekCleanup", () => {
  it("ticks the suggested ones and deletes only what stays ticked, highest first", async () => {
    api.scanSeekResumes.mockResolvedValue({
      tab: "t1",
      resumes: [
        { index: 1, name: "Jane - Resume.pdf", added: "Added 20 days ago", delete: true },
        { index: 2, name: "Jane - Resume.pdf", added: "Added 10 days ago", delete: true },
        { index: 3, name: "Jane - Resume.pdf", added: "Added just now", delete: false },
      ],
    });
    api.deleteSeekResume.mockResolvedValue({ deleted: ["Jane - Resume.pdf"], failed: [] });

    render(<SeekCleanup />);
    fireEvent.click(screen.getByRole("button", { name: /find old resumes/i }));
    const boxes = await screen.findAllByRole("checkbox");
    expect(boxes.map((b) => (b as HTMLInputElement).checked)).toEqual([true, true, false]);

    fireEvent.click(boxes[0]); // keep the oldest after all
    fireEvent.click(screen.getByRole("button", { name: /delete 1 ticked/i }));
    await waitFor(() => expect(api.deleteSeekResume).toHaveBeenCalledOnce());
    expect(api.deleteSeekResume).toHaveBeenCalledWith("t1", { index: 2, name: "Jane - Resume.pdf" });
  });
});
