/**
 * What's shown while the model is off reading the cache.
 *
 * A tool round can take a few seconds with nothing to show for it, and a bare
 * "Thinking…" gives you no idea whether anything is happening or which of your
 * data it went to look at. This does three things at once: a shimmer proving
 * the app is alive, the actual tool label so you can see it fetch your zones,
 * and a running clock once it has been a while.
 */
import { useEffect, useRef, useState } from "react";

/**
 * Cycled while waiting, in order, one every few seconds. Only ever shown when
 * no real tool label has arrived — a genuine "Reading recent activities" is
 * always better than a joke about it.
 */
const IDLE_WORDS = [
  "Thinking",
  "Reading the cache",
  "Counting minutes",
  "Checking zones",
  "Doing arithmetic",
  "Cross-referencing",
  "Second-guessing",
  "Nearly there",
];

const WORD_MS = 3200;

export function ToolLoader({ label }: { label?: string | null }) {
  const [tick, setTick] = useState(0);
  const [elapsed, setElapsed] = useState(0);
  const started = useRef(Date.now());

  useEffect(() => {
    const words = setInterval(() => setTick((t) => t + 1), WORD_MS);
    const clock = setInterval(
      () => setElapsed(Math.floor((Date.now() - started.current) / 1000)),
      1000,
    );
    return () => {
      clearInterval(words);
      clearInterval(clock);
    };
  }, []);

  const text = label || IDLE_WORDS[tick % IDLE_WORDS.length];

  return (
    <div style={{ display: "flex", alignItems: "center", gap: 11, fontSize: "var(--fs-base)" }}>
      <Pulse />
      <span className="shimmer">{text}…</span>
      {/* Only once it's long enough that you'd start to wonder. */}
      {elapsed >= 5 && (
        <span className="mono" style={{ fontSize: "var(--fs-caption)", color: "var(--faint)" }}>
          {elapsed}s
        </span>
      )}
    </div>
  );
}

/** Three dots breathing in sequence. Cheap, and reads as "working". */
function Pulse() {
  return (
    <span style={{ display: "inline-flex", gap: 4, flex: "none" }} aria-hidden="true">
      {[0, 1, 2].map((i) => (
        <span
          key={i}
          className="pulse-dot"
          style={{ animationDelay: `${i * 0.16}s` }}
        />
      ))}
    </span>
  );
}
