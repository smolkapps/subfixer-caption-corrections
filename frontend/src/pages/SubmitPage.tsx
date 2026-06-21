import { useMemo, useState } from "react";
import { submitCorrection, listCorrections, type Correction } from "../api";
import { diffWords } from "../lib/worddiff";
import { parseTimestamp, formatTimestamp } from "../lib/timestamp";
import { DiffView } from "../components/DiffView";

const FIXER_KEY = "subfixer.fixerName";

export function SubmitPage() {
  const [videoUrl, setVideoUrl] = useState("");
  const [timestamp, setTimestamp] = useState("");
  const [original, setOriginal] = useState("");
  const [corrected, setCorrected] = useState("");
  const [fixerName, setFixerName] = useState(
    () => localStorage.getItem(FIXER_KEY) ?? "",
  );
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [existing, setExisting] = useState<Correction[]>([]);

  const parsedSec = useMemo(() => parseTimestamp(timestamp), [timestamp]);
  const diff = useMemo(() => diffWords(original, corrected), [original, corrected]);

  const canSubmit =
    videoUrl.trim() !== "" &&
    parsedSec !== null &&
    corrected.trim() !== "" &&
    fixerName.trim() !== "" &&
    diff.wordsChanged > 0;

  async function loadExisting() {
    setError(null);
    if (videoUrl.trim() === "") {
      setError("Enter a video URL first.");
      return;
    }
    try {
      const list = await listCorrections(videoUrl.trim());
      setExisting(list);
      setStatus(`Found ${list.length} existing correction(s) for this video.`);
    } catch (e) {
      setError((e as Error).message);
    }
  }

  async function onSubmit(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    setStatus(null);
    if (parsedSec === null) {
      setError("Invalid timestamp.");
      return;
    }
    try {
      const saved = await submitCorrection({
        video_url: videoUrl.trim(),
        start_sec: parsedSec,
        original_text: original,
        corrected_text: corrected,
        fixer_name: fixerName.trim(),
      });
      localStorage.setItem(FIXER_KEY, fixerName.trim());
      setStatus(
        `Saved! You changed ${saved.words_changed} word(s) at ${formatTimestamp(
          saved.start_sec,
        )}.`,
      );
      setExisting((prev) => [saved, ...prev]);
      setOriginal("");
      setCorrected("");
    } catch (e) {
      setError((e as Error).message);
    }
  }

  return (
    <section>
      <h2>Submit a caption correction</h2>
      <form onSubmit={onSubmit} className="card">
        <label>
          Video URL
          <input
            type="url"
            value={videoUrl}
            placeholder="https://www.youtube.com/watch?v=..."
            onChange={(e) => setVideoUrl(e.target.value)}
            required
          />
        </label>

        <div className="row">
          <label>
            Timestamp (s or mm:ss)
            <input
              type="text"
              value={timestamp}
              placeholder="1:23"
              onChange={(e) => setTimestamp(e.target.value)}
              aria-invalid={timestamp !== "" && parsedSec === null}
            />
            {timestamp !== "" && parsedSec === null && (
              <small className="err">Invalid timestamp</small>
            )}
            {parsedSec !== null && (
              <small className="ok">= {formatTimestamp(parsedSec)}</small>
            )}
          </label>

          <label>
            Your fixer name
            <input
              type="text"
              value={fixerName}
              placeholder="caption_hero"
              onChange={(e) => setFixerName(e.target.value)}
              required
            />
          </label>
        </div>

        <label>
          Original / auto-generated caption
          <textarea
            value={original}
            onChange={(e) => setOriginal(e.target.value)}
            rows={3}
            placeholder="teh quick brown fox"
          />
        </label>

        <label>
          Your corrected caption
          <textarea
            value={corrected}
            onChange={(e) => setCorrected(e.target.value)}
            rows={3}
            placeholder="the quick brown fox"
          />
        </label>

        <div className="preview">
          <strong>Preview ({diff.wordsChanged} word(s) changed):</strong>
          <DiffView ops={diff.ops} />
        </div>

        <div className="actions">
          <button type="submit" disabled={!canSubmit}>
            Submit correction
          </button>
          <button type="button" onClick={loadExisting} className="secondary">
            Load existing for this video
          </button>
        </div>

        {status && <p className="status ok" role="status">{status}</p>}
        {error && <p className="status err" role="alert">{error}</p>}
      </form>

      {existing.length > 0 && (
        <div className="card">
          <h3>Corrections for this video</h3>
          <ul className="corrections">
            {existing.map((c) => (
              <li key={c.id}>
                <span className="ts">[{formatTimestamp(c.start_sec)}]</span>{" "}
                <span className="by">{c.fixer_name}</span> changed{" "}
                {c.words_changed} word(s):
                <DiffView ops={diffWords(c.original_text, c.corrected_text).ops} />
              </li>
            ))}
          </ul>
        </div>
      )}
    </section>
  );
}
