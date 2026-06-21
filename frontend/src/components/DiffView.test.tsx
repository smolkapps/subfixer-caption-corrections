import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { DiffView } from "./DiffView";
import { diffWords } from "../lib/worddiff";

describe("DiffView", () => {
  it("renders an empty-state message for no ops", () => {
    render(<DiffView ops={[]} />);
    expect(screen.getByText(/no content yet/i)).toBeInTheDocument();
  });

  it("marks inserted and deleted words distinctly", () => {
    const { ops } = diffWords("teh quick fox", "the quick fox");
    render(<DiffView ops={ops} />);
    // "teh" deleted, "the" inserted, "quick"/"fox" unchanged.
    const ins = screen.getAllByTestId("diff-ins");
    const del = screen.getAllByTestId("diff-del");
    expect(ins).toHaveLength(1);
    expect(del).toHaveLength(1);
    expect(ins[0]).toHaveTextContent("the");
    expect(del[0]).toHaveTextContent("teh");
  });

  it("renders only equals when nothing changed", () => {
    const { ops } = diffWords("same words", "same words");
    render(<DiffView ops={ops} />);
    expect(screen.queryAllByTestId("diff-ins")).toHaveLength(0);
    expect(screen.queryAllByTestId("diff-del")).toHaveLength(0);
    expect(screen.getByTestId("diff-view")).toHaveTextContent("same words");
  });
});
