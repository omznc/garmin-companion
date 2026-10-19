// The Ask threads for Expo Go: the same commands as the core, kept in memory.
import samples from "../../../../crates/garmin-api/fixtures/chat.sample.json";
import type { HistoryRow, Thread, ThreadMeta, Turn } from "./chatTypes";

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
    t.status === "running" && !(running && i === last) ? { ...t, status: "stopped" as const } : t,
  );
  return { ...row, running, turns };
}

type Args = {
  sessionId?: string;
  turnId?: string;
  title?: string;
  limit?: number;
  offset?: number;
  history?: HistoryRow[];
};
type Emit = (payload: object) => void;
type Handler = (a: Args, at: number, emit: Emit) => unknown;

// The turn is still in its thread. A deleted thread takes its turn with it.
const inThread = (sessionId: string, turn: Turn) =>
  rows.find((r) => r.sessionId === sessionId)?.turns.includes(turn) ?? false;

// A canned answer for the Ask screen, so it runs in Expo Go with no model.
const FIXTURE_ANSWER =
  "This is a sample answer, so the screen works with no setup.\n\nWith a model set up, the coach reads **your own Garmin data** first, then answers. Health comes first:\n\n- Sleep, resting heart rate and HRV\n- Recovery, stress and body battery\n- Activity of any sport, but only the sport your data shows\n\nA number that the data cannot support is left out, and the answer says why.";

let stopped = false;
// One turn at a time: a new turn stops the old one.
let run = 0;

async function fixtureTurn(sessionId: string, turnId: string, turn: Turn, emit: Emit) {
  const mine = ++run;
  const over = () => stopped || mine !== run || !inThread(sessionId, turn);
  const send = (payload: object) => emit({ ...payload, sessionId, turnId });
  const label = "Reading 14 days of recovery signals";
  const step = { callId: "fixture", label };
  send({ type: "tool", ...step, running: true, ok: true });
  turn.steps = [{ ...step, running: true, ok: null }];
  await new Promise((r) => setTimeout(r, 400));
  turn.steps = [{ ...step, running: false, ok: true }];
  send({ type: "tool", ...step, running: false, ok: true });
  for (const text of FIXTURE_ANSWER.split(/(?<= )/)) {
    if (over()) break;
    turn.answer += text;
    send({ type: "delta", text });
    await new Promise((r) => setTimeout(r, 40));
  }
  turn.status = over() ? "stopped" : "done";
  if (mine === run) setRunning(null);
  send({ type: "done", sources: [] });
}

const handlers = new Map<string, Handler>([
  [
    "chat_list",
    (a) => {
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
    },
  ],
  ["chat_get", (_a, at) => (rows[at] ? view(rows[at]) : null)],
  [
    "chat_rename",
    (a, at) => {
      // The same cap as the core.
      const title = a.title?.trim().slice(0, 200);
      if (!title) throw new Error("title is empty");
      if (rows[at]) rows[at].title = title;
      return {};
    },
  ],
  [
    "chat_delete",
    (a, at) => {
      if (a.sessionId === runningId) runningId = null;
      if (at >= 0) rows.splice(at, 1);
      return {};
    },
  ],
  [
    "ask_send",
    (a, _at, emit) => {
      const question = [...(a.history ?? [])].reverse().find((m) => m.role === "user")?.content;
      stopped = false;
      const turn = beginTurn(a.sessionId ?? "", question ?? "");
      setRunning(a.sessionId ?? null);
      void fixtureTurn(a.sessionId ?? "", a.turnId ?? "", turn, emit);
      return {};
    },
  ],
  [
    "ask_stop",
    () => {
      stopped = true;
      return {};
    },
  ],
]);

// The chat_* and ask_send/ask_stop commands. Other commands give undefined.
export function fixtureChat(cmd: string, args: unknown, emit: Emit): unknown {
  const a = (args ?? {}) as Args;
  return handlers.get(cmd)?.(
    a,
    rows.findIndex((r) => r.sessionId === a.sessionId),
    emit,
  );
}
