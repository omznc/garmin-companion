/**
 * The labels you put on your own sessions.
 *
 * Garmin has no tag concept to sync against, so these are local and stay local.
 * They exist because "how do my tempo sessions compare" is a question the data
 * cannot answer on its own — nothing in a Garmin export says which sessions you
 * *meant* as tempo — and because the coach on the Ask screen can read them.
 *
 * Deliberately quiet. This sits under a session's numbers, not above them, and
 * when nothing is tagged it is one word rather than an empty input box asking
 * to be filled in.
 */
import { useEffect, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { allTags, setActivityTags } from "../lib/api";
import { CloseIcon, NewIcon } from "../lib/icons";

/** Matches the ceiling the cache enforces, so the limit is visible before it bites. */
const MAX_TAGS = 12;
const MAX_CHARS = 32;

/** Suggestions offered under the input. More than this is a list, not a hint. */
const MAX_SUGGESTIONS = 6;

export function Tags({ activityId, tags }: { activityId: number; tags: string[] }) {
  const qc = useQueryClient();
  const [adding, setAdding] = useState(false);
  const [draft, setDraft] = useState("");
  const input = useRef<HTMLInputElement>(null);

  const known = useQuery({ queryKey: ["allTags"], queryFn: allTags });

  const save = useMutation({
    mutationFn: (next: string[]) => setActivityTags(activityId, next),
    onSuccess: (stored) => {
      // The analysis carries the tags and is fingerprinted against them, so a
      // tag change invalidates the written summary too — which is the point:
      // telling the coach a session was a tempo effort should change what it
      // says about it.
      qc.setQueryData(["activityTags", activityId], stored);
      void qc.invalidateQueries({ queryKey: ["activityAnalysis", activityId] });
      void qc.invalidateQueries({ queryKey: ["allTags"] });
    },
  });

  useEffect(() => {
    if (adding) input.current?.focus();
  }, [adding]);

  const full = tags.length >= MAX_TAGS;

  function add(raw: string) {
    const tag = raw.trim().toLowerCase().slice(0, MAX_CHARS);
    if (!tag || tags.includes(tag) || full) {
      setDraft("");
      return;
    }
    save.mutate([...tags, tag]);
    setDraft("");
  }

  function remove(tag: string) {
    save.mutate(tags.filter((t) => t !== tag));
  }

  // Tags already on this session are dropped from the suggestions — offering
  // one you already have does nothing when you press it.
  const suggestions = (known.data ?? [])
    .map((t) => t.tag)
    .filter((t) => !tags.includes(t) && (!draft || t.startsWith(draft.trim().toLowerCase())))
    .slice(0, MAX_SUGGESTIONS);

  return (
    <div style={{ display: "flex", alignItems: "center", gap: 10, flexWrap: "wrap" }}>
      <span className="eyebrow" style={{ marginRight: 4 }}>
        Tags
      </span>

      {tags.map((tag) => (
        <span
          key={tag}
          style={{
            display: "inline-flex",
            alignItems: "center",
            gap: 6,
            fontSize: "var(--fs-caption)",
            color: "var(--mut)",
            border: "1px solid var(--line)",
            borderRadius: 3,
            padding: "3px 6px 3px 9px",
          }}
        >
          {tag}
          <button
            className="quiet"
            onClick={() => remove(tag)}
            disabled={save.isPending}
            aria-label={`Remove the ${tag} tag`}
            title={`Remove the ${tag} tag`}
            style={{ display: "grid", placeItems: "center", color: "var(--faint)" }}
          >
            <CloseIcon size={11} aria-hidden />
          </button>
        </span>
      ))}

      {tags.length === 0 && !adding && (
        <span style={{ fontSize: "var(--fs-caption)", color: "var(--faint)" }}>
          None yet
        </span>
      )}

      {adding ? (
        <span style={{ display: "inline-flex", alignItems: "baseline", gap: 12 }}>
          <input
            ref={input}
            className="input-bare"
            value={draft}
            maxLength={MAX_CHARS}
            placeholder="tempo, long run, hills…"
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              // Comma as well as Enter: people type tags in lists, and the
              // alternative is a comma silently becoming part of one tag.
              if (e.key === "Enter" || e.key === ",") {
                e.preventDefault();
                add(draft);
              } else if (e.key === "Escape") {
                setDraft("");
                setAdding(false);
              }
            }}
            onBlur={() => {
              if (draft.trim()) add(draft);
              setAdding(false);
            }}
            style={{ fontSize: "var(--fs-caption)", width: 160 }}
          />
        </span>
      ) : (
        !full && (
          <button
            className="quiet"
            onClick={() => setAdding(true)}
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: 5,
              fontSize: "var(--fs-caption)",
              color: "var(--faint)",
            }}
          >
            <NewIcon size={11} aria-hidden />
            Add
          </button>
        )
      )}

      {/* Tags used elsewhere, so a second "tempo" session gets the same label
          rather than "tempo run" and a group of one. Only while typing —
          otherwise this row is a list of every tag on every screen. */}
      {adding &&
        suggestions.map((tag) => (
          <button
            key={tag}
            className="underlined"
            // `onMouseDown` rather than `onClick`: the input's blur fires first
            // and would unmount this before a click ever landed.
            onMouseDown={(e) => {
              e.preventDefault();
              add(tag);
            }}
            style={{ fontSize: "var(--fs-caption)", color: "var(--faint)" }}
          >
            {tag}
          </button>
        ))}

      {save.error != null && (
        <span style={{ fontSize: "var(--fs-caption)", color: "var(--warn)" }}>
          Couldn't save that tag.
        </span>
      )}
    </div>
  );
}
