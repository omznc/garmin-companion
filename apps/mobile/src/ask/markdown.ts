// A small markdown reader for an assistant answer. Port of the Farmer reader.
// The text arrives in parts while the answer streams, so every rule here
// must accept half of a construct and never throw.
// Constructs: paragraph, heading, list, pipe table, rule, code fence, inline
// code, bold, italic, link. Farmer directives (day citations, charts and
// follow-up chips) are not read here: they stay as plain text.

export type Align = "left" | "right" | "center";

export type Block =
  | { kind: "p"; text: string }
  | { kind: "h"; level: 1 | 2 | 3; text: string }
  | { kind: "ul"; items: string[] }
  | { kind: "ol"; items: string[] }
  | { kind: "table"; head: string[]; rows: string[][]; align: (Align | null)[] }
  | { kind: "hr" }
  | { kind: "code"; text: string; lang: string | null };

// A pipe row with both outer pipes. Strict, because a sentence can hold a pipe.
const isTableLine = (line: string) => /^\s*\|.*\|\s*$/.test(line);

// A pipe row once a table is known to be here.
const isTableRow = (line: string) => line.includes("|");

// `|---|:--:|---:|`: the line that makes the row above it a header.
const isTableDelimiter = (line: string) => {
  const trimmed = line.trim();
  if (/^\|(\s*:?-{2,}:?\s*\|)+$/.test(trimmed)) return true;
  const inner = trimmed.replace(/^\|/, "").replace(/\|$/, "");
  if (!inner.includes("|")) return false;
  return /^\s*:?-{2,}:?\s*(\|\s*:?-{2,}:?\s*)+$/.test(inner);
};

const startsTable = (line: string, next: string) => isTableRow(line) && isTableDelimiter(next);

const splitRow = (line: string) =>
  line
    .trim()
    .replace(/^\|/, "")
    .replace(/\|$/, "")
    .split("|")
    .map((cell) => cell.trim());

const alignOf = (spec: string): Align | null => {
  const trimmed = spec.trim();
  if (trimmed.startsWith(":") && trimmed.endsWith(":")) return "center";
  if (trimmed.endsWith(":")) return "right";
  if (trimmed.startsWith(":")) return "left";
  return null;
};

const takeTable = (lines: string[], start: number) => {
  const head = splitRow(lines[start] ?? "");
  const align = splitRow(lines[start + 1] ?? "").map(alignOf);
  let index = start + 2;
  const rows: string[][] = [];
  while (index < lines.length && isTableRow(lines[index] ?? "")) {
    rows.push(splitRow(lines[index] ?? ""));
    index += 1;
  }
  const block: Block = { kind: "table", head, rows, align };
  return { block, next: index };
};

// A fence whose body is only a pipe table, read as the table.
const fencedTable = (body: string[]): Block | null => {
  const lines = body.filter((line) => line.trim() !== "");
  if (lines.length < 2) return null;
  if (!startsTable(lines[0] ?? "", lines[1] ?? "")) return null;
  if (!lines.every(isTableRow)) return null;
  return takeTable(lines, 0).block;
};

const BULLET = /^\s*[-*]\s+/;
const NUMBERED = /^\s*\d+[.)]\s+/;
const HEADING = /^(#{1,3})\s+(.*)$/;
const RULE = /^\s*(-{3,}|\*{3,}|_{3,})\s*$/;

// A block read from the lines, and the index of the first line after it.
// A blank line gives `null`. `next` is always past the start line, or the
// loop would never end.
type Read = { block: Block | null; next: number };

// A fenced block, or null when the line at `start` opens no fence. An open
// fence runs to the end of the text that has arrived.
const readFence = (lines: string[], start: number): Read | null => {
  const fence = /^```([\w-]*)\s*$/.exec((lines[start] ?? "").trim());
  if (!fence) return null;
  const body: string[] = [];
  let index = start + 1;
  while (index < lines.length && !/^```/.test(lines[index] ?? "")) {
    body.push(lines[index] ?? "");
    index += 1;
  }
  const block = fencedTable(body) ?? {
    kind: "code",
    text: body.join("\n"),
    lang: fence[1] || null,
  };
  return { block, next: index + 1 };
};

const readHeading = (line: string): Block | null => {
  const heading = HEADING.exec(line);
  if (!heading) return null;
  const length = heading[1]?.length ?? 1;
  return {
    kind: "h",
    level: length === 3 ? 3 : length === 2 ? 2 : 1,
    text: heading[2] ?? "",
  };
};

// The run of list items from `start`, which the caller has matched.
const takeList = (lines: string[], start: number, pattern: RegExp, kind: "ol" | "ul"): Read => {
  const items: string[] = [];
  let index = start;
  while (index < lines.length && pattern.test(lines[index] ?? "")) {
    items.push((lines[index] ?? "").replace(pattern, ""));
    index += 1;
  }
  return { block: { kind, items }, next: index };
};

const endsParagraph = (line: string, next: string) =>
  line.trim() === "" ||
  BULLET.test(line) ||
  NUMBERED.test(line) ||
  /^#{1,3}\s+/.test(line) ||
  isTableLine(line) ||
  startsTable(line, next) ||
  /^```/.test(line);

const takeParagraph = (lines: string[], start: number): Read => {
  const paragraph: string[] = [];
  let index = start;
  while (index < lines.length && !endsParagraph(lines[index] ?? "", lines[index + 1] ?? "")) {
    paragraph.push(lines[index] ?? "");
    index += 1;
  }
  // A lone table header while the answer streams matches nothing above.
  // Take the line as its own paragraph, so the loop always moves on.
  if (paragraph.length === 0) {
    paragraph.push(lines[start] ?? "");
    index += 1;
  }
  return { block: { kind: "p", text: paragraph.join("\n") }, next: index };
};

const readBlock = (lines: string[], index: number): Read => {
  const line = lines[index] ?? "";
  const fence = readFence(lines, index);
  if (fence) return fence;
  if (startsTable(line, lines[index + 1] ?? "")) return takeTable(lines, index);
  const heading = readHeading(line);
  if (heading) return { block: heading, next: index + 1 };
  if (RULE.test(line)) return { block: { kind: "hr" }, next: index + 1 };
  if (NUMBERED.test(line)) return takeList(lines, index, NUMBERED, "ol");
  if (BULLET.test(line)) return takeList(lines, index, BULLET, "ul");
  if (line.trim() === "") return { block: null, next: index + 1 };
  return takeParagraph(lines, index);
};

// The blocks of an answer.
export const parseBlocks = (source: string): Block[] => {
  const blocks: Block[] = [];
  const lines = source.split("\n");
  let index = 0;
  while (index < lines.length) {
    const { block, next } = readBlock(lines, index);
    if (block) blocks.push(block);
    index = next;
  }
  return blocks;
};

export type Inline =
  | { kind: "text"; text: string }
  | { kind: "code"; text: string }
  | { kind: "bold"; text: string }
  | { kind: "italic"; text: string }
  // `href` is null for an address that is not a plain web link.
  | { kind: "link"; text: string; href: string | null };

// Only an https, http or mail link opens. The model can copy `javascript:`
// from data that it read.
export const safeHref = (href: string) =>
  /^(https?:\/\/|mailto:)/i.test(href.trim()) ? href.trim() : null;

const INLINE = /(`[^`]+`|\*\*[^*]+\*\*|\*[^*\n]+\*|\[[^\]\n]+\]\([^)\s]+\))/g;

// A span wrapped in `mark`, with `min` as the shortest whole part.
const wrapped = (part: string, mark: string, min: number) =>
  part.length > min && part.startsWith(mark) && part.endsWith(mark)
    ? part.slice(mark.length, -mark.length)
    : null;

const inlinePart = (part: string): Inline => {
  const code = wrapped(part, "`", 2);
  if (code !== null) return { kind: "code", text: code };
  const bold = wrapped(part, "**", 4);
  if (bold !== null) return { kind: "bold", text: bold };
  const italic = wrapped(part, "*", 2);
  if (italic !== null) return { kind: "italic", text: italic };
  const link = /^\[([^\]]+)\]\(([^)\s]+)\)$/.exec(part);
  if (link) return { kind: "link", text: link[1] ?? "", href: safeHref(link[2] ?? "") };
  return { kind: "text", text: part };
};

// The inline parts of a line, in one pass, so that no part can take another.
export const parseInline = (text: string): Inline[] =>
  text
    .split(INLINE)
    .filter((part) => part !== "")
    .map(inlinePart);

// A cell that is a quantity and nothing else: `7.25`, `-3`, `82%`.
const isNumeric = (cell: string) =>
  /^[-+]?[$€£]?\s?\d[\d\s,.]*\s?(%|h|hrs?|€|\$|£)?$/i.test(cell.trim());

// The alignment of each column: the delimiter's, else right for a column of
// numbers, else left.
export const columnAlign = (head: string[], rows: string[][], align: (Align | null)[]): Align[] => {
  const count = rows.reduce((widest, row) => Math.max(widest, row.length), head.length);
  return Array.from({ length: count }, (_, column) => {
    const given = align[column];
    if (given) return given;
    const cells = rows.map((row) => row[column]?.trim() ?? "").filter((cell) => cell !== "");
    return cells.length > 0 && cells.every(isNumeric) ? "right" : "left";
  });
};
