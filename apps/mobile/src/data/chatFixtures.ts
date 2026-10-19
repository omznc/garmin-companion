// The Ask threads for Expo Go: the same commands as the core, kept in memory.
import samples from "../../../../crates/garmin-api/fixtures/chat.sample.json";
import type { Thread, ThreadMeta, Turn } from "./chatTypes";

type Row = Omit<Thread, "running">;

const rows: Row[] = (samples as Row[]).map((r) => structuredClone(r));
let runningId: string | null = null;

// The title is the first question, cut on a word boundary to 80 characters.
export function makeTitle(question: string): string {
  const q = question.trim();
  if (q.length <= 80) return q;
  const cut = q.slice(0, 79);
  const at = cut.search(/\s\S*$/);
  return `${(at > 0 ? cut.slice(0, at) : cut).trimEnd()}…`;
}

export function setRunning(id: string | null) {
  runningId = id;
}

// Add a running turn. A new thread takes its title from the question.
export function beginTurn(sessionId: string, question: string): Turn {
  let row = rows.find((r) => r.sessionId === sessionId);
  if (!row) {
    row = { sessionId, title: makeTitle(question), updatedAt: "", turns: [] };
    rows.push(row);
  }
  const turn: Turn = {
    id: `${sessionId}:${row.turns.length}`,
    question,
    answer: "",
    steps: [],
    status: "running",
    error: null,
  };
  row.turns.push(turn);
  row.updatedAt = new Date().toISOString();
  return turn;
}

function view(row: Row): Thread {
  const running = row.sessionId === runningId;
  const last = row.turns.length - 1;
  const turns = row.turns.map((t, i) =>
    t.status === "running" && !(running && i === last)
      ? { ...t, status: "stopped" as const }
      : t,
  );
  return { ...row, running, turns };
}

export function fixtureChat(cmd: string, args: unknown): unknown {
  const a = (args ?? {}) as { sessionId?: string; title?: string; limit?: number; offset?: number };
  const at = rows.findIndex((r) => r.sessionId === a.sessionId);
  switch (cmd) {
    case "chat_list": {
      const start = a.offset ?? 0;
      const threads: ThreadMeta[] = [...rows]
        .sort((x, y) => y.updatedAt.localeCompare(x.updatedAt))
        .slice(start, start + (a.limit ?? 50))
        .map((r) => ({
          sessionId: r.sessionId,
          title: r.title,
          updatedAt: r.updatedAt,
          turnCount: r.turns.length,
          running: r.sessionId === runningId,
        }));
      return { threads };
    }
    case "chat_get": {
      const row = rows[at];
      return row ? view(row) : null;
    }
    case "chat_rename": {
      const title = a.title?.trim();
      if (!title) throw new Error("title is empty");
      const row = rows[at];
      if (row) row.title = title;
      return {};
    }
    case "chat_delete":
      if (a.sessionId === runningId) runningId = null;
      if (at >= 0) rows.splice(at, 1);
      return {};
    default:
      return undefined;
  }
}
