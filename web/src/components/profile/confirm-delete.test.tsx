import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { ConfirmDelete } from "./answers-manager";

afterEach(cleanup);

describe("ConfirmDelete", () => {
  it("asks once before deleting", () => {
    const onConfirm = vi.fn();
    render(<ConfirmDelete onConfirm={onConfirm} />);
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    expect(onConfirm).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Delete?" }));
    expect(onConfirm).toHaveBeenCalledOnce();
  });
});
