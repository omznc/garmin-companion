/**
 * Markdown for model answers.
 *
 * The coach writes prose with the occasional table of per-zone minutes, and
 * before this the raw asterisks and pipes were shown verbatim. Every element is
 * mapped onto the app's own design rather than left to browser defaults, so an
 * answer reads like the rest of the app instead of like a README.
 *
 * Tables get the most attention because they carry the numbers: hairline rules
 * instead of borders, and numeric cells set in the mono face and right-aligned
 * so columns of figures line up on the decimal.
 */
import type { ReactNode } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";

/** Cell contents that should be treated as a figure rather than a label. */
const NUMERIC = /^[\s+\-–]*[\d.,:]+\s*(%|bpm|km|kg|ms|spm|kcal|m|s|h|min|L)?\s*$/i;

function isNumeric(children: ReactNode): boolean {
  const text = flatten(children).trim();
  return text.length > 0 && NUMERIC.test(text);
}

/** Markdown cells arrive as nested nodes, so the text has to be gathered up. */
function flatten(node: ReactNode): string {
  if (node == null || typeof node === "boolean") return "";
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (Array.isArray(node)) return node.map(flatten).join("");
  const el = node as { props?: { children?: ReactNode } };
  return el.props ? flatten(el.props.children) : "";
}

const cellBase: React.CSSProperties = {
  padding: "9px 16px 9px 0",
  verticalAlign: "top",
  textAlign: "left",
};

const COMPONENTS: Components = {
  p: ({ children }) => <p style={{ margin: "0 0 16px" }}>{children}</p>,

  strong: ({ children }) => <strong style={{ fontWeight: 500 }}>{children}</strong>,
  em: ({ children }) => <em style={{ fontStyle: "italic" }}>{children}</em>,

  // The model shouldn't be shouting headings inside a chat answer, so these
  // stay close to body size and lean on the serif for hierarchy instead.
  h1: ({ children }) => <Heading size={22}>{children}</Heading>,
  h2: ({ children }) => <Heading size={19}>{children}</Heading>,
  h3: ({ children }) => <Heading size={17}>{children}</Heading>,
  h4: ({ children }) => <Heading size={16}>{children}</Heading>,

  // Bullets are drawn in CSS rather than per-item here, so an ordered list
  // keeps its numbers instead of getting a dot on top of them.
  ul: ({ children }) => <ul className="md-ul">{children}</ul>,
  ol: ({ children }) => <ol className="md-ol">{children}</ol>,

  a: ({ children, href }) => (
    <a href={href} target="_blank" rel="noreferrer">
      {children}
    </a>
  ),

  code: ({ children, className }) => {
    // Fenced blocks arrive with a language class; inline code has none.
    const block = Boolean(className);
    if (block) return <code className="mono">{children}</code>;
    return (
      <code
        className="mono"
        style={{
          fontSize: "0.9em",
          background: "var(--sel)",
          padding: "1px 5px",
          borderRadius: 3,
        }}
      >
        {children}
      </code>
    );
  },

  pre: ({ children }) => (
    <pre
      style={{
        margin: "0 0 16px",
        padding: "14px 16px",
        background: "var(--sel)",
        borderRadius: 4,
        overflowX: "auto",
        fontSize: 13,
        lineHeight: 1.6,
      }}
    >
      {children}
    </pre>
  ),

  blockquote: ({ children }) => (
    <blockquote
      style={{
        margin: "0 0 16px",
        paddingLeft: 16,
        borderLeft: "1px solid var(--line)",
        color: "var(--mut)",
      }}
    >
      {children}
    </blockquote>
  ),

  hr: () => <div className="rule" style={{ margin: "26px 0" }} />,

  // A wide table must scroll inside its own box rather than push the answer
  // column sideways.
  table: ({ children }) => (
    <div style={{ overflowX: "auto", margin: "0 0 20px" }}>
      <table
        style={{
          borderCollapse: "collapse",
          width: "100%",
          fontSize: 14,
          lineHeight: 1.5,
        }}
      >
        {children}
      </table>
    </div>
  ),

  thead: ({ children }) => <thead>{children}</thead>,

  th: ({ children, style }) => (
    <th
      style={{
        ...cellBase,
        ...style,
        borderBottom: "1px solid var(--line)",
        paddingBottom: 8,
        font: "400 10.5px/1 'Instrument Sans', sans-serif",
        letterSpacing: "0.11em",
        textTransform: "uppercase",
        color: "var(--faint)",
        whiteSpace: "nowrap",
        // Header alignment follows the column it labels.
        textAlign: (style?.textAlign as "left" | "right" | "center") ?? "left",
      }}
    >
      {children}
    </th>
  ),

  tr: ({ children }) => (
    <tr style={{ borderBottom: "1px solid var(--line2)" }}>{children}</tr>
  ),

  td: ({ children, style }) => {
    const numeric = isNumeric(children);
    return (
      <td
        className={numeric ? "mono" : undefined}
        style={{
          ...cellBase,
          ...style,
          // An explicit GFM alignment wins; otherwise figures go right so the
          // column reads as a column.
          textAlign: (style?.textAlign as "left" | "right" | "center") ??
            (numeric ? "right" : "left"),
          paddingRight: numeric ? 0 : cellBase.padding,
          whiteSpace: numeric ? "nowrap" : undefined,
        }}
      >
        {children}
      </td>
    );
  },
};

function Heading({ children, size }: { children: ReactNode; size: number }) {
  return (
    <div
      className="serif"
      style={{ fontSize: size, lineHeight: 1.25, margin: "22px 0 10px" }}
    >
      {children}
    </div>
  );
}

export function Markdown({ children }: { children: string }) {
  return (
    <ReactMarkdown remarkPlugins={[remarkGfm]} components={COMPONENTS}>
      {children}
    </ReactMarkdown>
  );
}
