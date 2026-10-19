import assert from "node:assert/strict";
import { describe, test } from "node:test";
import { type Block, columnAlign, parseBlocks, parseInline, safeHref } from "./markdown.ts";

const kinds = (blocks: Block[]) => blocks.map((block) => block.kind);

const SINK = `# Heading

A paragraph with **bold**, *italic*, \`code\` and [a link](https://example.com/).

- A bullet
- A second bullet

1. A numbered item
2. A second one

---

| Metric | Value |
|---|---:|
| Steps | 12000 |

\`\`\`ts
const x = 1;
\`\`\``;

describe("parseBlocks", () => {
  test("reads every construct", () => {
    const blocks = parseBlocks(SINK);
    assert.deepEqual(kinds(blocks), ["h", "p", "ul", "ol", "hr", "table", "code"]);
    const table = blocks[5];
    assert.deepEqual(table?.kind === "table" ? table.align : null, [null, "right"]);
  });

  test("a directive fence is plain code", () => {
    assert.deepEqual(kinds(parseBlocks("```farmer-next\nAsk more\n```")), ["code"]);
  });

  test("an open fence runs to the end", () => {
    assert.deepEqual(parseBlocks("```ts\nconst x"), [
      { kind: "code", text: "const x", lang: "ts" },
    ]);
  });

  test("a fence that holds only a table is the table", () => {
    assert.deepEqual(kinds(parseBlocks("```\n| A | B |\n|---|---|\n| 1 | 2 |\n```")), ["table"]);
  });

  test("a lone table header while streaming does not stop the parse", () => {
    assert.deepEqual(kinds(parseBlocks("| Metric | Value |")), ["p"]);
    assert.deepEqual(kinds(parseBlocks("| A | B |\n| 1 |")), ["p", "p"]);
  });
});

describe("parseInline", () => {
  test("reads each inline construct once", () => {
    assert.deepEqual(parseInline("**7h** with `x`, *this* or [docs](https://a.example)"), [
      { kind: "bold", text: "7h" },
      { kind: "text", text: " with " },
      { kind: "code", text: "x" },
      { kind: "text", text: ", " },
      { kind: "italic", text: "this" },
      { kind: "text", text: " or " },
      { kind: "link", text: "docs", href: "https://a.example" },
    ]);
  });

  test("a link that is not a web link does not open", () => {
    assert.equal(safeHref("javascript:alert(1)"), null);
    assert.equal(safeHref("mailto:a@b.example"), "mailto:a@b.example");
  });

  test("a day citation stays text", () => {
    assert.deepEqual(parseInline("[[2026-10-05]]"), [{ kind: "text", text: "[[2026-10-05]]" }]);
  });
});

describe("columnAlign", () => {
  test("sets a column of numbers to the right", () => {
    assert.deepEqual(
      columnAlign(
        ["Day", "Hours"],
        [
          ["Mon", "7.5h"],
          ["Tue", "8"],
        ],
        [null, null],
      ),
      ["left", "right"],
    );
  });
});
