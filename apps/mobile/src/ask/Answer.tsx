import { Fragment, useMemo } from "react";
import { Linking, Platform, Text as RNText, ScrollView, type TextStyle, View } from "react-native";
import { type Align, type Block, columnAlign, parseBlocks, parseInline } from "./markdown";
import { type ChatTheme, useChatTheme } from "./theme";

const MONO = Platform.select({ ios: "Menlo", default: "monospace" });

const BODY: TextStyle = { fontSize: 16, lineHeight: 24 };
const SMALL: TextStyle = { fontSize: 14, lineHeight: 20 };

const openLink = (href: string) => {
  void Linking.openURL(href).catch(() => undefined);
};

// The inline parts of one line, as nested text.
function Inline({ text, style }: { text: string; style?: TextStyle }) {
  const theme = useChatTheme();
  const parts = useMemo(() => parseInline(text), [text]);
  return (
    <RNText selectable style={[BODY, { color: theme.text }, style]}>
      {parts.map((part, index) => {
        // The parts come again from one string on each streamed delta, so
        // the position is the identity.
        const key = `${index}-${part.kind}`;
        switch (part.kind) {
          case "code":
            return (
              <RNText
                key={key}
                style={{ fontFamily: MONO, fontSize: 14, backgroundColor: theme.field }}
              >
                {part.text}
              </RNText>
            );
          case "bold":
            return (
              <RNText key={key} style={{ fontWeight: "600" }}>
                {part.text}
              </RNText>
            );
          case "italic":
            return (
              <RNText key={key} style={{ fontStyle: "italic" }}>
                {part.text}
              </RNText>
            );
          case "link": {
            const { href } = part;
            if (!href) return <Fragment key={key}>{part.text}</Fragment>;
            return (
              <RNText
                key={key}
                accessibilityRole="link"
                style={{ color: theme.accent, textDecorationLine: "underline" }}
                onPress={() => openLink(href)}
              >
                {part.text}
              </RNText>
            );
          }
          case "text":
            return <Fragment key={key}>{part.text}</Fragment>;
        }
      })}
    </RNText>
  );
}

// The width of a column, from its longest cell. A long column wraps.
const columnWidth = (head: string, cells: string[]) => {
  const longest = cells.reduce((widest, cell) => Math.max(widest, cell.length), head.length);
  return Math.min(Math.max(longest * 8 + 32, 64), 240);
};

function Cell({
  content,
  header,
  width,
  align,
  theme,
}: {
  content: string;
  header: boolean;
  width: number;
  align: Align;
  theme: ChatTheme;
}) {
  return (
    <View
      style={{
        // Each column is at least its width. A narrow table shares the rest
        // in proportion. A wide table keeps its widths and scrolls.
        flexBasis: width,
        flexGrow: width,
        flexShrink: 0,
        paddingHorizontal: 12,
        paddingVertical: 8,
      }}
    >
      {content.trim() === "" && !header ? (
        <RNText style={[SMALL, { color: theme.faint }]}>-</RNText>
      ) : (
        <Inline
          text={content}
          style={{
            ...SMALL,
            fontWeight: header ? "500" : undefined,
            color: header ? theme.muted : theme.text,
            textAlign: align,
            fontVariant: ["tabular-nums"],
          }}
        />
      )}
    </View>
  );
}

function Table({ block }: { block: Extract<Block, { kind: "table" }> }) {
  const theme = useChatTheme();
  const { head, rows } = block;
  const aligns = columnAlign(head, rows, block.align);
  const widths = aligns.map((_, column) =>
    columnWidth(
      head[column] ?? "",
      rows.map((row) => row[column] ?? ""),
    ),
  );
  const line = (cells: string[], header: boolean) =>
    aligns.map((align, column) => (
      <Cell
        // The columns have a fixed order: the position is the column.
        // biome-ignore lint/suspicious/noArrayIndexKey: see above
        key={column}
        content={cells[column] ?? ""}
        header={header}
        width={widths[column] ?? 64}
        align={align}
        theme={theme}
      />
    ));
  return (
    <ScrollView
      horizontal
      showsHorizontalScrollIndicator={false}
      style={{ borderRadius: 12, borderWidth: 1, borderColor: theme.line }}
      contentContainerStyle={{ flexGrow: 1 }}
    >
      <View style={{ flexGrow: 1 }}>
        <View
          style={{
            flexDirection: "row",
            backgroundColor: theme.header,
            borderBottomWidth: 1,
            borderBottomColor: theme.line,
          }}
        >
          {line(head, true)}
        </View>
        {rows.map((row, rowIndex) => (
          <View
            // A row grows while it streams, so the position is the identity.
            // biome-ignore lint/suspicious/noArrayIndexKey: see above
            key={rowIndex}
            style={{
              flexDirection: "row",
              borderBottomWidth: rowIndex === rows.length - 1 ? 0 : 1,
              borderBottomColor: theme.line,
            }}
          >
            {line(row, false)}
          </View>
        ))}
      </View>
    </ScrollView>
  );
}

function List({ block }: { block: Extract<Block, { kind: "ul" | "ol" }> }) {
  const theme = useChatTheme();
  return (
    <View style={{ gap: 4 }}>
      {block.items.map((item, itemIndex) => (
        <View
          // The list comes again on each delta, so the position is the identity.
          // biome-ignore lint/suspicious/noArrayIndexKey: see above
          key={itemIndex}
          style={{ flexDirection: "row", gap: 8 }}
        >
          <View style={{ minWidth: 16 }}>
            {block.kind === "ol" ? (
              <RNText style={[BODY, { color: theme.muted }]}>{`${itemIndex + 1}.`}</RNText>
            ) : (
              <View
                style={{
                  width: 5,
                  height: 5,
                  borderRadius: 3,
                  marginTop: 10,
                  marginLeft: 4,
                  backgroundColor: theme.muted,
                }}
              />
            )}
          </View>
          <View style={{ flex: 1 }}>
            <Inline text={item} />
          </View>
        </View>
      ))}
    </View>
  );
}

function Code({ text }: { text: string }) {
  const theme = useChatTheme();
  return (
    <ScrollView
      horizontal
      style={{ borderRadius: 10, backgroundColor: theme.field }}
      contentContainerStyle={{ padding: 12 }}
    >
      <RNText
        selectable
        style={{ fontFamily: MONO, fontSize: 13, lineHeight: 19, color: theme.text }}
      >
        {text}
      </RNText>
    </ScrollView>
  );
}

function BlockView({ block }: { block: Block }) {
  const theme = useChatTheme();
  switch (block.kind) {
    case "p":
      return <Inline text={block.text} />;
    case "h":
      return (
        <Inline
          text={block.text}
          style={{ fontSize: 16, lineHeight: 24, fontWeight: "600", marginTop: 4 }}
        />
      );
    case "ul":
    case "ol":
      return <List block={block} />;
    case "table":
      return <Table block={block} />;
    case "hr":
      return <View style={{ height: 1, backgroundColor: theme.line }} />;
    case "code":
      return <Code text={block.text} />;
  }
}

// One answer, drawn from the markdown that the model wrote.
export function Answer({ text }: { text: string }) {
  const blocks = useMemo(() => parseBlocks(text), [text]);
  if (!text.trim()) return null;
  return (
    <View style={{ gap: 12 }}>
      {blocks.map((block, index) => (
        // A block is keyed by its kind and its place, so a block that grows
        // while it streams keeps its view.
        <BlockView key={`${block.kind}-${index}`} block={block} />
      ))}
    </View>
  );
}
