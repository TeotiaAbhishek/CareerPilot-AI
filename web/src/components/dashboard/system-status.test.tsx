import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { SystemStatus } from "./system-status";

afterEach(() => vi.unstubAllGlobals());

describe("SystemStatus", () => {
  it("shows API and database online when health is ok", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        new Response(JSON.stringify({ status: "ok", database: "ok", version: "0.1.0" })),
      ),
    );

    render(<SystemStatus />);

    expect(await screen.findAllByText("Online")).toHaveLength(2);
  });

  it("shows both down when the API can't be reached", async () => {
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new TypeError("fetch failed")));

    render(<SystemStatus />);

    expect(await screen.findAllByText("Down")).toHaveLength(2);
  });
});
