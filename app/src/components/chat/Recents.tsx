/**
 * Earlier conversations, in a drawer over the one you're reading.
 *
 * They used to be a list at the bottom of the Ask screen, visible only while
 * the screen was empty — which meant the way back to something you asked
 * yesterday was to first throw away what you were asking now. A conversation
 * you can't reach from inside a conversation isn't history, it's an archive.
 */
import { useEffect, useRef } from "react";
import { useInfiniteQuery, useQueryClient } from "@tanstack/react-query";
import { chatSessions, deleteChatSession, type ChatSessionMeta } from "../../lib/api";
import { since } from "../../lib/format";
import { DeleteIcon, NewIcon } from "../../lib/icons";

/** How many past conversations each scroll fetches. */
const PAGE = 15;

export function Recents({
  openId,
  onOpen,
  onNew,
  onClose,
}: {
  /** The conversation currently on screen, marked in the list. */
  openId: string | null;
  onOpen: (sessionId: string) => void;
  onNew: () => void;
  onClose: () => void;
}) {
  const qc = useQueryClient();
  const sentinel = useRef<HTMLDivElement>(null);

  const q = useInfiniteQuery({
    queryKey: ["chatSessions"],
    queryFn: ({ pageParam }) => chatSessions(PAGE, pageParam),
    initialPageParam: 0,
    // A short page means the end; otherwise ask for everything past what we hold.
    getNextPageParam: (last, all) =>
      last.length < PAGE ? undefined : all.reduce((n, p) => n + p.length, 0),
  });

  const { hasNextPage, isFetchingNextPage, fetchNextPage } = q;

  useEffect(() => {
    const el = sentinel.current;
    if (!el || !hasNextPage) return;
    const io = new IntersectionObserver((entries) => {
      if (entries[0].isIntersecting && !isFetchingNextPage) void fetchNextPage();
    });
    io.observe(el);
    return () => io.disconnect();
  }, [hasNextPage, isFetchingNextPage, fetchNextPage]);

  const sessions = q.data?.pages.flat() ?? [];

  async function remove(id: string) {
    await deleteChatSession(id);
    await qc.invalidateQueries({ queryKey: ["chatSessions"] });
  }

  return (
    <>
      <div className="drawer-head">
        <div className="eyebrow">Conversations</div>
        <button
          type="button"
          className="quiet"
          style={{
            display: "inline-flex",
            alignItems: "center",
            gap: 6,
            fontSize: "var(--fs-caption)",
          }}
          onClick={() => {
            onNew();
            onClose();
          }}
        >
          <NewIcon size={13} style={{ flex: "none" }} aria-hidden />
          New
        </button>
      </div>

      <div className="drawer-list">
        {sessions.length === 0 ? (
          <p style={{ fontSize: "var(--fs-small)", color: "var(--faint)", margin: "6px 0" }}>
            Nothing yet. Ask something and it will be here afterwards.
          </p>
        ) : (
          sessions.map((s) => (
            <Past
              key={s.sessionId}
              session={s}
              open={s.sessionId === openId}
              onOpen={() => {
                onOpen(s.sessionId);
                onClose();
              }}
              onDelete={() => void remove(s.sessionId)}
            />
          ))
        )}
        {/* Sits below the last row; crossing it pulls the next page in. */}
        <div ref={sentinel} style={{ height: 1 }} />
        {isFetchingNextPage && (
          <div style={{ fontSize: "var(--fs-caption)", color: "var(--faint)", padding: "14px 0" }}>
            Loading…
          </div>
        )}
      </div>
    </>
  );
}

function Past({
  session,
  open,
  onOpen,
  onDelete,
}: {
  session: ChatSessionMeta;
  open: boolean;
  onOpen: () => void;
  onDelete: () => void;
}) {
  return (
    <div className="row-group">
      <button className="row" style={{ flex: 1, minWidth: 0 }} onClick={onOpen}>
        <span
          style={{
            flex: 1,
            minWidth: 0,
            // Wrapped, not truncated. A title is what the conversation was
            // about, and half of it plus an ellipsis is not enough to tell two
            // days of asking about the same run apart. `anywhere` so a long
            // unbroken word breaks rather than pushing the row sideways.
            overflowWrap: "anywhere",
            // The one on screen, marked by weight rather than by a badge — the
            // list is one column wide and has no room for a second signal.
            color: open ? "var(--fg)" : undefined,
          }}
        >
          {session.title}
        </span>
        <span
          style={{
            fontSize: "var(--fs-caption)",
            color: "var(--faint)",
            flex: "none",
            whiteSpace: "nowrap",
          }}
        >
          {since(session.updatedAt)}
        </span>
      </button>
      {/* Last in the row and vertically centred against it. The slot is always
          there — it keeps the row's right edge steady — and where there is a
          pointer the icon waits for it, since deleting is never the reason you
          opened this list.

          It used to wait on a React `mouseenter` and `visibility: hidden`, which
          hid it from more than the eye: `visibility: hidden` is not focusable,
          so a keyboard could never reach it, and a touchscreen has no hover, so
          on a phone there was no way to delete a conversation at all. Now it is
          only the opacity that is conditional, and only where hovering exists —
          see `.row-trail`. */}
      <button
        className="quiet row-trail"
        title="Delete this conversation"
        aria-label="Delete this conversation"
        onClick={onDelete}
      >
        <DeleteIcon size={15} aria-hidden />
      </button>
    </div>
  );
}
