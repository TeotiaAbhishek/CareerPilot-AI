import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { LabelControl } from "./label-control";

afterEach(cleanup);

describe("LabelControl", () => {
  it("selects a label", () => {
    const onChange = vi.fn();
    render(<LabelControl value={null} onChange={onChange} />);
    fireEvent.click(screen.getByRole("radio", { name: "Apply" }));
    expect(onChange).toHaveBeenCalledWith("apply");
  });

  it("clears the label when you click the selected one", () => {
    const onChange = vi.fn();
    render(<LabelControl value="maybe" onChange={onChange} />);
    expect(screen.getByRole("radio", { name: "Maybe" })).toHaveAttribute("aria-checked", "true");
    fireEvent.click(screen.getByRole("radio", { name: "Maybe" }));
    expect(onChange).toHaveBeenCalledWith(null);
  });
});
