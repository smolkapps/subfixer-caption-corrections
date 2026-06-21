// Typed client for the SubFixer backend API.

export interface WordOp {
  op: "equal" | "insert" | "delete";
  word: string;
}

export interface DiffResult {
  ops: WordOp[];
  words_inserted: number;
  words_deleted: number;
  words_changed: number;
}

export interface Correction {
  id: string;
  storage_key: string;
  video_id: string;
  start_sec: number;
  original_text: string;
  corrected_text: string;
  words_changed: number;
  fixer_id: string;
  fixer_name: string;
  created_at: number;
}

export interface LeaderboardEntry {
  rank: number;
  fixer_id?: string;
  display_name: string;
  corrections: number;
  words_changed: number;
}

export interface NewCorrection {
  video_url: string;
  start_sec: number;
  original_text: string;
  corrected_text: string;
  fixer_name: string;
}

const BASE = "/api";

async function handle<T>(res: Response): Promise<T> {
  if (!res.ok) {
    let message = `request failed (${res.status})`;
    try {
      const body = await res.json();
      if (body && typeof body.error === "string") message = body.error;
    } catch {
      // non-JSON error body; keep the default message
    }
    throw new Error(message);
  }
  return (await res.json()) as T;
}

export async function submitCorrection(input: NewCorrection): Promise<Correction> {
  const res = await fetch(`${BASE}/corrections`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(input),
  });
  return handle<Correction>(res);
}

export async function listCorrections(videoUrl: string): Promise<Correction[]> {
  const res = await fetch(`${BASE}/corrections?url=${encodeURIComponent(videoUrl)}`);
  const data = await handle<{ corrections: Correction[] }>(res);
  return data.corrections;
}

export async function getLeaderboard(limit = 100): Promise<LeaderboardEntry[]> {
  const res = await fetch(`${BASE}/leaderboard?limit=${limit}`);
  const data = await handle<{ leaderboard: LeaderboardEntry[] }>(res);
  return data.leaderboard;
}

export async function previewDiff(
  originalText: string,
  correctedText: string,
): Promise<DiffResult> {
  const res = await fetch(`${BASE}/preview-diff`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ original_text: originalText, corrected_text: correctedText }),
  });
  return handle<DiffResult>(res);
}

export async function setAnonymity(
  fixerName: string,
  anonymous: boolean,
): Promise<{ fixer_name: string; anonymous: boolean }> {
  const res = await fetch(`${BASE}/anonymity`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ fixer_name: fixerName, anonymous }),
  });
  return handle(res);
}
