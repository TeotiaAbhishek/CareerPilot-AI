import { cleanup, render, screen } from "@testing-library/react";
import { Briefcase } from "lucide-react";
import { afterEach, describe, expect, it } from "vitest";

import { StatCard } from "./stat-card";

afterEach(cleanup);

describe("StatCard", () => {
  it("is a link to the list when given an href", () => {
    render(<StatCard label="Saved" value={3} hint="Bookmarked" icon={Briefcase} href="/jobs?status=saved" />);
    expect(screen.getByRole("link", { name: /Saved: 3/ })).toHaveAttribute("href", "/jobs?status=saved");
  });

  it("is plain text without one", () => {
    render(<StatCard label="Saved" value={null} hint="Bookmarked" icon={Briefcase} />);
    expect(screen.queryByRole("link")).toBeNull();
    expect(screen.getByText("—")).toBeInTheDocument();
  });
});
