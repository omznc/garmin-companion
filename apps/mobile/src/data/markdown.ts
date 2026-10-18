// Simple markdown for the coach's answers. The same rules as `markdown.rs` of the desktop app:
// paragraphs, headings, bold, inline code, bullet and numbered lists, fenced code.
// The parser is forgiving, because an answer is drawn while it streams.
export type Block =
  | { kind: "paragraph"; text: string }
  | { kind: "heading"; text: string }
  | { kind: "item"; marker: string; text: string }
  | { kind: "code"; text: string };

export type Span = { text: string; bold?: boolean; code?: boolean };

const BULLET = /^[-*+] (.*)$/;
const NUMBERED = /^(\d{1,3})\. (.*)$/;
const HEADING = /^#{1,6} (.*)$/;

export function blocks(source: string): Block[] {
  const out: Block[] = [];
  let para: string[] = [];
  let code: string[] | null = null;
  const flush = () => {
    if (para.length) out.push({ kind: "paragraph", text: para.join(" ") });
    para = [];
  };
  for (const line of source.split("\n")) {
    const trimmed = line.trim();
    if (code) {
      if (trimmed.startsWith("```")) {
        out.push({ kind: "code", text: code.join("\n") });
        code = null;
      } else code.push(line);
      continue;
    }
    const bullet = BULLET.exec(trimmed);
    const numbered = NUMBERED.exec(trimmed);
    const heading = HEADING.exec(trimmed);
    if (trimmed.startsWith("```")) {
      flush();
      code = [];
    } else if (!trimmed) flush();
    else if (heading) {
      flush();
      out.push({ kind: "heading", text: (heading[1] ?? "").trim() });
    } else if (bullet) {
      flush();
      out.push({ kind: "item", marker: "•", text: (bullet[1] ?? "").trim() });
    } else if (numbered) {
      flush();
      out.push({ kind: "item", marker: `${numbered[1]}.`, text: (numbered[2] ?? "").trim() });
    } else para.push(trimmed);
  }
  flush();
  if (code) out.push({ kind: "code", text: code.join("\n") });
  return out;
}

// A marker with no partner stays in the text.
export function inline(line: string): Span[] {
  const spans: Span[] = [];
  let plain = "";
  const push = () => {
    if (plain) spans.push({ text: plain });
    plain = "";
  };
  let rest = line;
  while (rest) {
    const open = rest.startsWith("**") ? "**" : rest.startsWith("`") ? "`" : "";
    const body = rest.slice(open.length);
    const end = open ? body.indexOf(open) : -1;
    if (open && end > 0) {
      push();
      spans.push(
        open === "**"
          ? { text: body.slice(0, end), bold: true }
          : { text: body.slice(0, end), code: true },
      );
      rest = body.slice(end + open.length);
    } else if (open) {
      plain += open;
      rest = body;
    } else {
      plain += rest.charAt(0);
      rest = rest.slice(1);
    }
  }
  push();
  return spans;
}
