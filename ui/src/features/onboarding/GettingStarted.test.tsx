import { beforeEach, describe, expect, it } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { GettingStarted } from "./GettingStarted";
import { installDefaultResponses, resetStore } from "@/test/helpers";
import { useKeel } from "@/state/store";

describe("GettingStarted", () => {
  beforeEach(() => {
    installDefaultResponses();
    resetStore();
  });

  it("renders nothing when the tour is closed", () => {
    render(<GettingStarted />);
    expect(screen.queryByText("Getting started")).toBeNull();
    expect(screen.queryByText("Step 1 of 4")).toBeNull();
  });

  it("starts on the first step and advances with Continue", () => {
    useKeel.setState({ gettingStartedOpen: true });
    render(<GettingStarted />);
    expect(screen.getByText("Step 1 of 4")).toBeTruthy();
    expect(screen.getByText("Browse your requests")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(screen.getByText("Step 2 of 4")).toBeTruthy();
    expect(screen.getByText("Switch environments")).toBeTruthy();
  });

  it("Skip closes the tour and marks it done", () => {
    useKeel.setState({ gettingStartedOpen: true });
    render(<GettingStarted />);
    fireEvent.click(screen.getByRole("button", { name: "Skip" }));
    expect(useKeel.getState().gettingStartedOpen).toBe(false);
    expect(localStorage.getItem("keel.gettingStarted")).toBe("done");
  });

  it("Continue on the last step finishes the tour and marks it done", () => {
    useKeel.setState({ gettingStartedOpen: true });
    render(<GettingStarted />);
    const cont = () => screen.getByRole("button", { name: "Continue" });
    fireEvent.click(cont());
    fireEvent.click(cont());
    fireEvent.click(cont());
    expect(screen.getByText("Step 4 of 4")).toBeTruthy();
    expect(screen.getByText("Share with Git")).toBeTruthy();

    fireEvent.click(cont());
    expect(useKeel.getState().gettingStartedOpen).toBe(false);
    expect(localStorage.getItem("keel.gettingStarted")).toBe("done");
  });

  it("shows the send shortcut hint on the send step", () => {
    useKeel.setState({ gettingStartedOpen: true });
    render(<GettingStarted />);
    const cont = () => screen.getByRole("button", { name: "Continue" });
    fireEvent.click(cont());
    fireEvent.click(cont());
    expect(screen.getByText("Send and test")).toBeTruthy();
    expect(screen.getByText(/to send the active request/)).toBeTruthy();
  });

  it("resets to the first step when reopened from the palette", () => {
    useKeel.setState({ gettingStartedOpen: true });
    const { rerender } = render(<GettingStarted />);
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(screen.getByText("Step 2 of 4")).toBeTruthy();

    act(() => {
      useKeel.setState({ gettingStartedOpen: false });
    });
    rerender(<GettingStarted />);
    act(() => {
      useKeel.getState().openGettingStarted();
    });
    rerender(<GettingStarted />);
    expect(screen.getByText("Step 1 of 4")).toBeTruthy();
  });
});
