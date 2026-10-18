//! Simple markdown for the coach's answers: paragraphs, headings, bold, inline
//! code, bullet and numbered lists, and fenced code.
//!
//! The parser is forgiving, because an answer is drawn while it streams. An
//! open `**` or a code fence with no end is text or code until the rest comes.
//! Links, tables, italics and nesting are not read: they show as typed.

use std::ops::Range;

use gpui::{
    div, prelude::*, px, relative, AnyElement, FontWeight, HighlightStyle, SharedString, StyledText,
};

use crate::theme::{Theme, FS_MD, FS_SMALL};
use crate::ui::{MONO, SANS};

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Paragraph(String),
    Heading(String),
    /// `marker` is `•` or `3.`.
    Item {
        marker: String,
        text: String,
    },
    Code(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mark {
    Bold,
    Code,
}

/// Split an answer into blocks.
pub fn blocks(source: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let mut para: Vec<&str> = Vec::new();
    let mut code: Option<Vec<&str>> = None;
    let flush = |para: &mut Vec<&str>, out: &mut Vec<Block>| {
        if !para.is_empty() {
            out.push(Block::Paragraph(para.join(" ")));
            para.clear();
        }
    };
    for line in source.lines() {
        if let Some(lines) = code.as_mut() {
            if line.trim_start().starts_with("```") {
                out.push(Block::Code(lines.join("\n")));
                code = None;
            } else {
                lines.push(line);
            }
            continue;
        }
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            flush(&mut para, &mut out);
            code = Some(Vec::new());
        } else if trimmed.is_empty() {
            flush(&mut para, &mut out);
        } else if let Some(text) = heading(trimmed) {
            flush(&mut para, &mut out);
            out.push(Block::Heading(text.to_string()));
        } else if let Some((marker, text)) = item(trimmed) {
            flush(&mut para, &mut out);
            out.push(Block::Item { marker, text });
        } else {
            para.push(trimmed);
        }
    }
    flush(&mut para, &mut out);
    if let Some(lines) = code {
        out.push(Block::Code(lines.join("\n")));
    }
    out
}

fn heading(line: &str) -> Option<&str> {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    let rest = line[hashes..].strip_prefix(' ')?;
    (1..=6).contains(&hashes).then_some(rest.trim())
}

fn item(line: &str) -> Option<(String, String)> {
    for bullet in ["- ", "* ", "+ "] {
        if let Some(rest) = line.strip_prefix(bullet) {
            return Some(("\u{2022}".into(), rest.trim().into()));
        }
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    let rest = line[digits..].strip_prefix(". ")?;
    (digits > 0 && digits < 4).then(|| (format!("{}.", &line[..digits]), rest.trim().into()))
}

/// The plain text of a line, and the marked ranges inside it. A marker with no
/// partner stays in the text.
pub fn inline(line: &str) -> (String, Vec<(Range<usize>, Mark)>) {
    let mut text = String::new();
    let mut marks = Vec::new();
    let mut rest = line;
    while !rest.is_empty() {
        let (open, mark) = if rest.starts_with("**") {
            ("**", Mark::Bold)
        } else if rest.starts_with('`') {
            ("`", Mark::Code)
        } else {
            // A lone `*` is text. Both markers are ASCII, so `end` is a boundary.
            let end = rest.find(['*', '`']).unwrap_or(rest.len()).max(1);
            text.push_str(&rest[..end]);
            rest = &rest[end..];
            continue;
        };
        let body = &rest[open.len()..];
        match body.find(open).filter(|&i| i > 0) {
            Some(i) => {
                let start = text.len();
                text.push_str(&body[..i]);
                marks.push((start..text.len(), mark));
                rest = &body[i + open.len()..];
            }
            None => {
                text.push_str(open);
                rest = body;
            }
        }
    }
    (text, marks)
}

fn styled(line: &str, t: &Theme) -> StyledText {
    let (text, marks) = inline(line);
    let runs = marks.into_iter().map(|(range, mark)| {
        let style = match mark {
            Mark::Bold => HighlightStyle {
                font_weight: Some(FontWeight::SEMIBOLD),
                ..Default::default()
            },
            Mark::Code => HighlightStyle {
                background_color: Some(t.sel),
                ..Default::default()
            },
        };
        (range, style)
    });
    StyledText::new(SharedString::from(text)).with_highlights(runs)
}

fn render_block(block: &Block, t: &Theme) -> AnyElement {
    match block {
        Block::Paragraph(text) => div()
            .line_height(relative(1.6))
            .child(styled(text, t))
            .into_any_element(),
        Block::Heading(text) => div()
            .mt(px(6.0))
            .font_weight(FontWeight::SEMIBOLD)
            .child(styled(text, t))
            .into_any_element(),
        Block::Item { marker, text } => div()
            .flex()
            .gap(px(10.0))
            .items_start()
            .line_height(relative(1.6))
            .child(
                div()
                    .flex_none()
                    .w(px(18.0))
                    .text_color(t.faint)
                    .child(marker.clone()),
            )
            .child(div().flex_1().min_w_0().child(styled(text, t)))
            .into_any_element(),
        Block::Code(text) => div()
            .px(px(14.0))
            .py(px(12.0))
            .rounded(px(6.0))
            .bg(t.sel)
            .font_family(MONO)
            .text_size(px(FS_SMALL - 0.5))
            .line_height(relative(1.5))
            .whitespace_nowrap()
            .overflow_hidden()
            .child(text.clone())
            .into_any_element(),
    }
}

/// The answer, drawn as blocks one gap apart. The items of one list sit closer.
pub fn render(source: &str, t: &Theme) -> impl IntoElement {
    let blocks = blocks(source);
    let mut out: Vec<AnyElement> = Vec::new();
    let mut at = 0;
    while at < blocks.len() {
        let items = blocks[at..]
            .iter()
            .take_while(|b| matches!(b, Block::Item { .. }))
            .count();
        if items == 0 {
            out.push(render_block(&blocks[at], t));
            at += 1;
        } else {
            let list = div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .children(blocks[at..at + items].iter().map(|b| render_block(b, t)));
            out.push(list.into_any_element());
            at += items;
        }
    }
    div()
        .flex()
        .flex_col()
        .gap(px(14.0))
        .font_family(SANS)
        .text_size(px(FS_MD))
        .text_color(t.fg)
        .children(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn para(s: &str) -> Block {
        Block::Paragraph(s.into())
    }

    #[test]
    fn paragraphs_split_at_a_blank_line_and_join_inside() {
        assert_eq!(blocks("a\nb\n\nc"), vec![para("a b"), para("c")]);
    }

    #[test]
    fn lists_and_headings_are_blocks_of_their_own() {
        let got = blocks("## Sleep\n- one\n- two\n1. first\n12. next\ntext");
        assert_eq!(got[0], Block::Heading("Sleep".into()));
        assert_eq!(
            got[1],
            Block::Item {
                marker: "\u{2022}".into(),
                text: "one".into()
            }
        );
        assert_eq!(
            got[3],
            Block::Item {
                marker: "1.".into(),
                text: "first".into()
            }
        );
        assert_eq!(
            got[4],
            Block::Item {
                marker: "12.".into(),
                text: "next".into()
            }
        );
        assert_eq!(got[5], para("text"));
    }

    #[test]
    fn a_fence_with_no_end_is_code_so_far() {
        assert_eq!(
            blocks("hi\n```\nlet x = 1;\nmore"),
            vec![para("hi"), Block::Code("let x = 1;\nmore".into())]
        );
        assert_eq!(
            blocks("```\na\n```\nb"),
            vec![Block::Code("a".into()), para("b")]
        );
    }

    #[test]
    fn a_hash_with_no_space_is_text() {
        assert_eq!(blocks("#hashtag"), vec![para("#hashtag")]);
        assert_eq!(blocks("1.5 km"), vec![para("1.5 km")]);
    }

    #[test]
    fn bold_and_code_are_marked_and_the_markers_leave() {
        let (text, marks) = inline("a **bold** and `code` end");
        assert_eq!(text, "a bold and code end");
        assert_eq!(marks, vec![(2..6, Mark::Bold), (11..15, Mark::Code)]);
    }

    #[test]
    fn an_open_marker_stays_in_the_text() {
        assert_eq!(inline("a **bol").0, "a **bol");
        assert_eq!(inline("2 * 3 = 6").0, "2 * 3 = 6");
        assert_eq!(inline("`x").0, "`x");
        assert_eq!(inline("****").0, "****");
    }

    #[test]
    fn accented_text_does_not_panic() {
        let (text, marks) = inline("čšž **ž** `č`");
        assert_eq!(text, "čšž ž č");
        assert_eq!(marks.len(), 2);
    }
}
