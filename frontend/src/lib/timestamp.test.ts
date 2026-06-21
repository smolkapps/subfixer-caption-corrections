import { describe, it, expect } from "vitest";
import { parseTimestamp, formatTimestamp } from "./timestamp";

describe("parseTimestamp", () => {
  it("parses plain seconds", () => {
    expect(parseTimestamp("83")).toBe(83);
    expect(parseTimestamp("0")).toBe(0);
    expect(parseTimestamp("83.7")).toBe(83); // floored
  });

  it("parses mm:ss", () => {
    expect(parseTimestamp("1:23")).toBe(83);
    expect(parseTimestamp("0:05")).toBe(5);
    expect(parseTimestamp("10:00")).toBe(600);
  });

  it("parses hh:mm:ss", () => {
    expect(parseTimestamp("1:02:03")).toBe(3723);
    expect(parseTimestamp("0:00:30")).toBe(30);
  });

  it("rejects malformed input", () => {
    expect(parseTimestamp("")).toBeNull();
    expect(parseTimestamp("   ")).toBeNull();
    expect(parseTimestamp("abc")).toBeNull();
    expect(parseTimestamp("-5")).toBeNull();
    expect(parseTimestamp("1:2:3:4")).toBeNull();
    expect(parseTimestamp("1:99")).toBeNull(); // seconds >= 60
    expect(parseTimestamp("99:99")).toBeNull(); // minute >= 60
  });

  it("rejects timestamps past the 24h cap", () => {
    expect(parseTimestamp("90000")).toBeNull();
    expect(parseTimestamp("25:00:00")).toBeNull();
  });
});

describe("formatTimestamp", () => {
  it("formats under an hour as m:ss", () => {
    expect(formatTimestamp(83)).toBe("1:23");
    expect(formatTimestamp(5)).toBe("0:05");
  });

  it("formats over an hour as h:mm:ss", () => {
    expect(formatTimestamp(3723)).toBe("1:02:03");
  });

  it("round-trips with parseTimestamp", () => {
    for (const sec of [0, 5, 83, 600, 3723]) {
      expect(parseTimestamp(formatTimestamp(sec))).toBe(sec);
    }
  });
});
