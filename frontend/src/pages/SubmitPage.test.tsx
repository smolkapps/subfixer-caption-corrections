import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SubmitPage } from "./SubmitPage";
import * as api from "../api";

vi.mock("../api", () => ({
  submitCorrection: vi.fn(),
  listCorrections: vi.fn(),
}));

const mockedSubmit = vi.mocked(api.submitCorrection);

function makeCorrection(over: Partial<api.Correction> = {}): api.Correction {
  return {
    id: "c1",
    storage_key: "dQw4w9WgXcQ@83",
    video_id: "dQw4w9WgXcQ",
    start_sec: 83,
    original_text: "teh fox",
    corrected_text: "the fox",
    words_changed: 1,
    fixer_id: "f1",
    fixer_name: "tester",
    created_at: 0,
    ...over,
  };
}

describe("SubmitPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
  });

  it("disables submit until the form is valid and a word actually changed", async () => {
    const user = userEvent.setup();
    render(<SubmitPage />);
    const submit = screen.getByRole("button", { name: /submit correction/i });
    expect(submit).toBeDisabled();

    await user.type(
      screen.getByLabelText(/video url/i),
      "https://youtu.be/dQw4w9WgXcQ",
    );
    await user.type(screen.getByLabelText(/timestamp/i), "1:23");
    await user.type(screen.getByLabelText(/your fixer name/i), "tester");
    await user.type(screen.getByLabelText(/original/i), "teh fox");
    // Identical correction -> still disabled (no change).
    await user.type(screen.getByLabelText(/your corrected caption/i), "teh fox");
    expect(submit).toBeDisabled();

    // Now make a real change.
    await user.clear(screen.getByLabelText(/your corrected caption/i));
    await user.type(screen.getByLabelText(/your corrected caption/i), "the fox");
    expect(submit).toBeEnabled();
  });

  it("submits the parsed timestamp and shows a success status", async () => {
    mockedSubmit.mockResolvedValue(makeCorrection());
    const user = userEvent.setup();
    render(<SubmitPage />);

    await user.type(
      screen.getByLabelText(/video url/i),
      "https://youtu.be/dQw4w9WgXcQ",
    );
    await user.type(screen.getByLabelText(/timestamp/i), "1:23");
    await user.type(screen.getByLabelText(/your fixer name/i), "tester");
    await user.type(screen.getByLabelText(/original/i), "teh fox");
    await user.type(screen.getByLabelText(/your corrected caption/i), "the fox");

    await user.click(screen.getByRole("button", { name: /submit correction/i }));

    await waitFor(() => expect(mockedSubmit).toHaveBeenCalledTimes(1));
    expect(mockedSubmit).toHaveBeenCalledWith({
      video_url: "https://youtu.be/dQw4w9WgXcQ",
      start_sec: 83, // 1:23 parsed
      original_text: "teh fox",
      corrected_text: "the fox",
      fixer_name: "tester",
    });
    expect(await screen.findByRole("status")).toHaveTextContent(/saved/i);
    // fixer name persisted for next time
    expect(localStorage.getItem("subfixer.fixerName")).toBe("tester");
  });

  it("surfaces a server error to the user", async () => {
    mockedSubmit.mockRejectedValue(new Error("no-op correction"));
    const user = userEvent.setup();
    render(<SubmitPage />);

    await user.type(
      screen.getByLabelText(/video url/i),
      "https://youtu.be/dQw4w9WgXcQ",
    );
    await user.type(screen.getByLabelText(/timestamp/i), "5");
    await user.type(screen.getByLabelText(/your fixer name/i), "tester");
    await user.type(screen.getByLabelText(/original/i), "a b");
    await user.type(screen.getByLabelText(/your corrected caption/i), "a c");

    await user.click(screen.getByRole("button", { name: /submit correction/i }));
    expect(await screen.findByRole("alert")).toHaveTextContent(/no-op/i);
  });
});
