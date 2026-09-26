import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { TimelinePane } from "./TimelinePane";

describe("TimelinePane", () => {
  it("shows an empty state", () => {
    render(<TimelinePane timeline={[]} />);
    expect(screen.getByText("No timeline events")).toBeTruthy();
  });

  it("renders phase and message for each event", () => {
    render(
      <TimelinePane
        timeline={[
          { ts: "2026-01-01T00:00:00Z", phase: "request", message: "GET /users" },
          { ts: "not-a-date", phase: "error", message: "timed out" },
        ]}
      />,
    );
    expect(screen.getByText("request")).toBeTruthy();
    expect(screen.getByText("GET /users")).toBeTruthy();
    expect(screen.getByText("error")).toBeTruthy();
    expect(screen.getByText("timed out")).toBeTruthy();
    expect(screen.getByText("not-a-date")).toBeTruthy();
  });
});
