//! Window chrome: the drag strip, the three window buttons and the resize
//! edges. From `app/src/components/WindowChrome.tsx`.
//!
//! The window has no system frame, as in the current app. On Linux the app
//! asks for client side decorations, and draws the buttons only while the
//! compositor leaves them to the app. macOS keeps its own traffic lights.

use crate::scale::px;
use gpui::{
    div, prelude::*, AnyElement, CursorStyle, Decorations, Div, MouseButton, ResizeEdge, Stateful,
    Window, WindowControlArea,
};

use crate::icons::{icon_hover, Icon};
use crate::theme::Theme;

/// The height of the title bar and of the strip that drags the window.
pub const STRIP: f32 = 38.0;
/// The width of the grab strip along each window edge.
const GRAB: f32 = 7.0;

/// Whether the app draws the three buttons.
fn buttons_are_ours(window: &Window) -> bool {
    if cfg!(target_os = "macos") {
        return false;
    }
    if cfg!(target_os = "linux") {
        return matches!(window.window_decorations(), Decorations::Client { .. });
    }
    true
}

/// Make an element drag the window. A double click zooms it. A child that
/// handles the mouse down itself, such as a button, keeps its click.
pub fn draggable(bar: Stateful<Div>) -> Stateful<Div> {
    bar.window_control_area(WindowControlArea::Drag)
        .on_mouse_down(MouseButton::Left, |event, window, _| {
            if event.click_count == 2 {
                window.zoom_window();
            } else {
                window.start_window_move();
            }
        })
}

/// The strip that drags the window, over a screen that has no title bar.
pub fn drag_strip() -> impl IntoElement {
    draggable(
        div()
            .id("drag-strip")
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(px(STRIP)),
    )
}

/// The window buttons in the top right corner of a screen with no title bar.
pub fn corner_controls(window: &Window, t: &Theme) -> Option<AnyElement> {
    let row = controls(window, t)?;
    Some(
        div()
            .absolute()
            .top_0()
            .right_0()
            .h(px(STRIP))
            .pr(px(6.0))
            .flex()
            .items_center()
            .child(row)
            .into_any_element(),
    )
}

fn button(
    id: &'static str,
    glyph: Icon,
    size: f32,
    area: WindowControlArea,
    hover_colour: gpui::Hsla,
    t: &Theme,
    action: fn(&mut Window),
) -> impl IntoElement {
    let (mutd, hover) = (t.mutd, t.hover);
    div()
        .id(id)
        .group(id)
        .flex()
        .items_center()
        .justify_center()
        .size(px(28.0))
        .rounded(px(6.0))
        .cursor_pointer()
        .window_control_area(area)
        .hover(move |s| s.bg(hover))
        // Mouse down, and claimed: the strip under the button starts a window
        // move on the same event.
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            cx.stop_propagation();
            action(window);
        })
        .child(icon_hover(glyph, size, t, mutd, Some((id, hover_colour))))
}

/// The three buttons in a row, or nothing where the system draws them.
pub fn controls(window: &Window, t: &Theme) -> Option<AnyElement> {
    if !buttons_are_ours(window) {
        return None;
    }
    let maximised = window.is_maximized();
    let (zoom_icon, zoom_size) = if maximised {
        (Icon::Restore, 13.0)
    } else {
        (Icon::Maximise, 12.0)
    };
    Some(
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(2.0))
            .child(button(
                "win-min",
                Icon::Minimise,
                13.0,
                WindowControlArea::Min,
                t.fg,
                t,
                |w| w.minimize_window(),
            ))
            .child(button(
                "win-max",
                zoom_icon,
                zoom_size,
                WindowControlArea::Max,
                t.fg,
                t,
                |w| w.zoom_window(),
            ))
            .child(button(
                "win-close",
                Icon::Close,
                13.0,
                WindowControlArea::Close,
                t.acc,
                t,
                |w| w.remove_window(),
            ))
            .into_any_element(),
    )
}

/// The eight invisible grab strips around the window.
///
/// Under system decorations there are none. A tiled window gets none on the
/// tiled edges, because it cannot grow there.
pub fn resize_handles(window: &Window) -> Vec<AnyElement> {
    let Decorations::Client { tiling } = window.window_decorations() else {
        return Vec::new();
    };
    let g = px(GRAB);
    let mut out = Vec::new();
    let mut edge = |id: &'static str,
                    edge: ResizeEdge,
                    cursor: CursorStyle,
                    place: fn(gpui::Div, gpui::Pixels) -> gpui::Div,
                    tiled: bool| {
        if tiled {
            return;
        }
        out.push(
            place(div().absolute(), g)
                .id(id)
                .cursor(cursor)
                // Occlude: the title bar under the strip gets no mouse down, so
                // its window move does not replace the resize.
                .occlude()
                .on_mouse_down(MouseButton::Left, move |_, window, _| {
                    window.start_window_resize(edge)
                })
                .into_any_element(),
        );
    };
    edge(
        "rs-top",
        ResizeEdge::Top,
        CursorStyle::ResizeUpDown,
        |d, g| d.top_0().left_0().right_0().h(g),
        tiling.top,
    );
    edge(
        "rs-bottom",
        ResizeEdge::Bottom,
        CursorStyle::ResizeUpDown,
        |d, g| d.bottom_0().left_0().right_0().h(g),
        tiling.bottom,
    );
    edge(
        "rs-left",
        ResizeEdge::Left,
        CursorStyle::ResizeLeftRight,
        |d, g| d.left_0().top_0().bottom_0().w(g),
        tiling.left,
    );
    edge(
        "rs-right",
        ResizeEdge::Right,
        CursorStyle::ResizeLeftRight,
        |d, g| d.right_0().top_0().bottom_0().w(g),
        tiling.right,
    );
    // The corners come last, so they sit above the edges that they overlap.
    edge(
        "rs-tl",
        ResizeEdge::TopLeft,
        CursorStyle::ResizeUpLeftDownRight,
        |d, g| d.top_0().left_0().w(g * 1.5).h(g * 1.5),
        tiling.top || tiling.left,
    );
    edge(
        "rs-tr",
        ResizeEdge::TopRight,
        CursorStyle::ResizeUpRightDownLeft,
        |d, g| d.top_0().right_0().w(g * 1.5).h(g * 1.5),
        tiling.top || tiling.right,
    );
    edge(
        "rs-bl",
        ResizeEdge::BottomLeft,
        CursorStyle::ResizeUpRightDownLeft,
        |d, g| d.bottom_0().left_0().w(g * 1.5).h(g * 1.5),
        tiling.bottom || tiling.left,
    );
    edge(
        "rs-br",
        ResizeEdge::BottomRight,
        CursorStyle::ResizeUpLeftDownRight,
        |d, g| d.bottom_0().right_0().w(g * 1.5).h(g * 1.5),
        tiling.bottom || tiling.right,
    );
    out
}
