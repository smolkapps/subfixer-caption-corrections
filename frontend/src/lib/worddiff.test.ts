import { describe, it, expect } from "vitest";
import { diffWords, tokenize } from "./worddiff";

describe("tokenize", () => {
  it("splits on whitespace and drops empties", () => {
    expect(tokenize("  hello   world \n foo ")).toEqual(["hello", "world", "foo"]);
    expect(tokenize("")).toEqual([]);
    expect(tokenize("   ")).toEqual([]);
  });
});

describe("diffWords", () => {
  it("reports no change for identical text", () => {
    const d = diffWords("the quick brown fox", "the quick brown fox");
    expect(d.wordsChanged).toBe(0);
    expect(d.ops.every((o) => o.op === "equal")).toBe(true);
  });

  it("counts a single substitution as one changed word", () => {
    const d = diffWords("teh quick brown fox", "the quick brown fox");
    expect(d.wordsInserted).toBe(1);
    expect(d.wordsDeleted).toBe(1);
    expect(d.wordsChanged).toBe(1);
  });

  it("counts pure insertions", () => {
    const d = diffWords("hello world", "hello there big world");
    expect(d.wordsInserted).toBe(2);
    expect(d.wordsDeleted).toBe(0);
    expect(d.wordsChanged).toBe(2);
  });

  it("counts pure deletions", () => {
    const d = diffWords("um hello uh world", "hello world");
    expect(d.wordsDeleted).toBe(2);
    expect(d.wordsInserted).toBe(0);
    expect(d.wordsChanged).toBe(2);
  });

  it("uses max(inserted, deleted) for substitutions of differing length", () => {
    const d = diffWords("foo bar baz", "one two three baz");
    expect(d.wordsInserted).toBe(3);
    expect(d.wordsDeleted).toBe(2);
    expect(d.wordsChanged).toBe(3);
  });

  it("handles empty original (all insertions)", () => {
    const d = diffWords("", "brand new caption");
    expect(d.wordsInserted).toBe(3);
    expect(d.wordsChanged).toBe(3);
  });

  it("handles empty correction (all deletions)", () => {
    const d = diffWords("delete all of this", "");
    expect(d.wordsDeleted).toBe(4);
    expect(d.wordsChanged).toBe(4);
  });

  it("preserves the corrected word order in ops", () => {
    const d = diffWords("a c", "a b c");
    const corrected = d.ops
      .filter((o) => o.op !== "delete")
      .map((o) => o.word);
    expect(corrected).toEqual(["a", "b", "c"]);
  });

  it("treats punctuation/casing as real changes", () => {
    const d = diffWords("hello world", "Hello, world.");
    expect(d.wordsChanged).toBe(2);
  });
});
