/**
 * The chat screen.
 *
 * It reads as a chat now — you type at the bottom, the conversation runs
 * downwards, and the newest thing is always nearest your hands. What it isn't is
 * a clone: there are no avatars, no assistant bubble and no per-message toolbar,
 * because the answer is prose about your training and this app already knows how
 * to set prose. The mechanics come from every other chat application; the
 * typography stays this one's.
 *
 * The parts live elsewhere. `useChat` owns a conversation, `Thread` draws it,
 * `Composer` is the docked box, `Recents` is the drawer. What's left here is the
 * screen's own business: which questions to suggest, where the scroll should be,
 * and what to say when no model has been chosen yet.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { Link, useSearch } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { chatConfig } from "../lib/api";
import { useChat } from "../lib/useChat";
import { scroller } from "../lib/scroller";
import { Composer, type ComposerHandle } from "../components/chat/Composer";
import { Recents } from "../components/chat/Recents";
import { Thread } from "../components/chat/Thread";
import { Drawer } from "../components/Drawer";
import { Empty, ErrorNote, Loading, PageHeader } from "../components/ui";
import { greeting } from "../lib/greeting";
import { NewIcon, PinIcon, UnpinIcon } from "../lib/icons";
import { IS_MOBILE } from "../lib/platform";

/**
 * Openers for a conversation that hasn't started, drawn from before the model
 * has anything to go on. It's a pool rather than a list because three fixed
 * questions in a fixed order stop being suggestions after the second visit —
 * `sample` takes three per visit, so the row proposes something you didn't ask
 * last time. Once an answer exists the model's own follow-ups take the slot.
 */
const OPENERS = [
  "Am I recovered enough to go hard today?",
  "How much of my last five runs was above Z2?",
  "Compare my last three runs.",
  "Is my cadence improving?",
  "Am I drifting back into Z5?",
  "How long was my longest easy run this month?",
  "What does my HRV trend say about this week?",
  "Is my resting heart rate moving?",
  "How did I sleep before my best run?",
  "What should this week's long run look like?",
  "Am I running more than last month?",
  "Which run this month was best executed?",
  "How much of my week was easy versus hard?",
  "What's holding my VO2 max back?",
];

/** Suggested slots. Pinned questions get their own, up to `MAX_PINS`. */
const SUGGESTED = 3;
const MAX_PINS = 6;

/** Pinned questions, per machine — they're a personal shortlist, not app data. */
const PINS_KEY = "garmin-companion:ask-pins";

/** How far off the bottom you can be and still count as being at it. */
const STICK_SLOP = 90;

/** How long the opening screen takes to get out of the way. Matches `--dur-slow`
 *  in the stylesheet; both are here so neither is a surprise. */
const HERO_EXIT = 180;

/**
 * The pinned shortlist.
 *
 * Read once on mount and written through on every change: the list is short,
 * only this screen touches it, and losing it to a failed write would be worse
 * than the write costing a millisecond.
 */
function usePins() {
  const [pins, setPins] = useState<string[]>(() => {
    try {
      const raw = JSON.parse(localStorage.getItem(PINS_KEY) ?? "[]");
      if (!Array.isArray(raw)) return [];
      return raw.filter((x): x is string => typeof x === "string").slice(0, MAX_PINS);
    } catch {
      return [];
    }
  });

  function write(next: string[]) {
    setPins(next);
    try {
      localStorage.setItem(PINS_KEY, JSON.stringify(next));
    } catch {
      // A full or blocked store costs the pins on next launch, nothing here.
    }
  }

  return {
    pins,
    full: pins.length >= MAX_PINS,
    toggle: (q: string) => {
      if (pins.includes(q)) write(pins.filter((p) => p !== q));
      else if (pins.length < MAX_PINS) write([...pins, q]);
    },
  };
}

/** `n` distinct items, in random order. Used for the openers, so unseeded. */
function sample<T>(xs: readonly T[], n: number): T[] {
  const pool = [...xs];
  for (let i = pool.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [pool[i], pool[j]] = [pool[j], pool[i]];
  }
  return pool.slice(0, n);
}

export function Ask() {
  const config = useQuery({ queryKey: ["chatConfig"], queryFn: chatConfig });
  // No follow-up suggestions asked for: the row that displayed them is now only
  // drawn on the empty screen, where by definition there is no answer to follow
  // up on. Leaving it on would be a second model request per turn — billed, on a
  // hosted provider — for a row nobody can see. `useChat` still knows how, so
  // putting the row back is this flag and the `empty` gate on `showChips`.
  const chat = useChat({ followups: false });
  const { pins, full, toggle } = usePins();
  /**
   * A question handed over from another screen arrives in the box rather than
   * already sent. Sleep is the screen that does it, and what it hands over is a
   * starting point — the useful version of that question usually has a clause
   * of your own on the end, and there is no way to add one to a turn that has
   * already gone.
   *
   * Read once, as the initial state, so editing it doesn't fight the URL and
   * a re-render can't put the original back under your cursor.
   */
  const seeded = useSearch({ from: "/ask" }).q;
  const [draft, setDraft] = useState(seeded ?? "");
  const [recents, setRecents] = useState(false);
  /** Whether the conversation has been scrolled away from its bottom. */
  const [away, setAway] = useState(false);
  // Drawn once per visit rather than per render, or every keystroke would
  // reshuffle the row underneath the pointer.
  const [openers] = useState(() => sample(OPENERS, SUGGESTED + MAX_PINS));

  /**
   * The opening screen, pinned to where it was, for as long as it takes to go.
   *
   * `empty` flips on the frame you press send — dropped from the tree there,
   * the whole header vanishes between two frames while the first turn fades up
   * into the hole it left. So the pixels it occupied are measured on the way
   * out and redrawn as a fixed copy that fades, over the top of the thread that
   * has already taken the space. Fading it in place instead would hold its
   * height open for the length of the fade, and the collapse this exists to
   * soften would simply happen a beat later.
   */
  const hero = useRef<HTMLDivElement>(null);
  /** The box, for the one moment the box can't know about. See "New" below. */
  const composer = useRef<ComposerHandle>(null);
  const [ghost, setGhost] = useState<{
    top: number;
    left: number;
    width: number;
    height: number;
  } | null>(null);

  // Cleared from here rather than from the send, so asking a second question
  // before the first has finished fading doesn't leave one pinned to the screen.
  useEffect(() => {
    if (!ghost) return;
    const t = setTimeout(() => setGhost(null), HERO_EXIT);
    return () => clearTimeout(t);
  }, [ghost]);

  /**
   * Whether new content should pull the view down with it.
   *
   * True while you are at the bottom, false the moment you scroll up — which is
   * the whole of the behaviour every chat application has and this one didn't.
   * Reading back through an answer while the next paragraph streams in must not
   * yank the page out from under you, and getting back to the live end must not
   * mean a long scroll.
   */
  const stick = useRef(true);

  useEffect(() => {
    const el = scroller();
    const onScroll = () => {
      const bottom = el.scrollHeight - el.scrollTop - el.clientHeight;
      stick.current = bottom < STICK_SLOP;
      setAway(!stick.current);
    };
    onScroll();
    el.addEventListener("scroll", onScroll, { passive: true });
    return () => el.removeEventListener("scroll", onScroll);
  }, []);

  const toBottom = useCallback((behavior: ScrollBehavior = "smooth") => {
    const el = scroller();
    el.scrollTo({ top: el.scrollHeight, behavior });
    stick.current = true;
    setAway(false);
  }, []);

  // Every beat of a turn is a reason to follow it down: prose arriving, a tool
  // row landing, a question appearing. Only while stuck — see above.
  const beat = `${chat.history.length}:${chat.pending?.length ?? -1}:${chat.steps.length}:${chat.asking.length}`;
  useEffect(() => {
    if (stick.current) toBottom(chat.pending === null ? "smooth" : "auto");
    // A streaming answer arrives a few characters at a time; smooth-scrolling
    // each one queues animations faster than they run and the view lags behind
    // the text. Jump for those, ease for the discrete events.
  }, [beat, toBottom, chat.pending]);

  // Reopening a conversation lands at its live end, not at its opening question.
  const opened = chat.sessionId;
  useEffect(() => {
    if (opened) toBottom("auto");
  }, [opened, toBottom]);

  if (config.isLoading) return <Loading label="Checking your model settings" />;

  const ready = config.data?.provider && config.data.model;
  if (!ready) {
    return (
      <div className="screen">
        <Empty
          title="Choose a model first."
          body="Answers come from a model you point this at — the built-in coach, OpenRouter with your own key, or a local Ollama. None is configured yet, and nothing is sent anywhere until one is."
          action={
            <Link className="cta" to="/settings">
              Open settings
            </Link>
          }
        />
      </div>
    );
  }

  const empty = chat.history.length === 0 && chat.pending === null;
  // Pinned questions come first and keep their place; this visit's openers fill
  // in behind them, minus anything already pinned so the row never offers the
  // same question twice.
  const suggested = openers.filter((q) => !pins.includes(q)).slice(0, SUGGESTED);
  // Only on the empty screen. The row is what to ask when you don't know what to
  // ask; once there is a conversation on the page, the thing to ask next comes
  // out of what was just said, and a rank of generic questions under it is the
  // app talking over the answer you are still reading.
  //
  // Held for the ghost's beat on the way out, and marked while it is: the row
  // sits inside the composer, so dropping it the frame you send shortens the
  // dock and jerks the box down under your hands as you let go of it.
  const hasChips = pins.length > 0 || suggested.length > 0;
  const showChips = (empty || ghost !== null) && hasChips;
  const chipsLeaving = !empty && ghost !== null;
  /** A question is on screen and the turn is waiting on it. */
  const blocked = chat.asking.some((a) => a.answers === null);

  /**
   * Send, and photograph the opening screen on its way out.
   *
   * Every send goes through here, from the box and from a chip alike. The
   * measurement has to happen now, while the hero is still on screen and `empty`
   * is still true — one render later React has swapped it for the thread and
   * there is nothing left to measure.
   */
  function ask(q: string) {
    const r = hero.current?.getBoundingClientRect();
    if (r) setGhost({ top: r.top, left: r.left, width: r.width, height: r.height });
    void chat.send(q);
    // Sending is an explicit request to be at the live end, whatever you were
    // reading a moment ago — as true of a chip as of the box, which is half the
    // reason both come through here.
    stick.current = true;
  }

  return (
    <>
      {empty ? (
        <div className="chat-hero" ref={hero}>
          <Hero />
        </div>
      ) : (
        <Thread
          history={chat.history}
          pending={chat.pending}
          blocks={chat.blocks}
          steps={chat.steps}
          drafting={chat.drafting}
          asking={chat.asking}
          onAnswer={(callId, answers) => void chat.answer(callId, answers)}
          onSaved={chat.markSaved}
          onDraftSaved={chat.markDrafting}
        />
      )}

      {/* The screen you just left, on its way out. Out of the document's flow
          and out of the accessible tree: it is the same words that were read a
          moment ago, and nothing here can be clicked. */}
      {ghost && !empty && (
        <div className="chat-hero chat-hero-ghost" style={ghost} aria-hidden>
          <Hero />
        </div>
      )}

      {chat.error && (
        <div style={{ paddingBottom: "calc(var(--composer-h) + 26px)" }}>
          <ErrorNote error={chat.error} />
        </div>
      )}

      {/* Kept mounted and hidden rather than unmounted, so it leaves the way it
          arrived. Dropped from the tree it vanished on the frame you reached the
          bottom, which is the one moment the eye is on it — a thing that fades in
          from below has to fade back down, or the screen reads as glitching
          rather than as settling. `visibility` in the hidden state keeps it off
          the tab order while it isn't offering anything. */}
      {!empty && (
        <button
          type="button"
          className="jump-pill"
          data-away={away || undefined}
          onClick={() => toBottom()}
        >
          Jump to latest
        </button>
      )}

      <Composer
        value={draft}
        onChange={setDraft}
        onSend={() => {
          const q = draft;
          setDraft("");
          ask(q);
        }}
        onStop={chat.stop}
        busy={chat.busy}
        blocked={blocked}
        // This screen is the box. Not on a phone, where taking the caret raises
        // the keyboard over the greeting before anyone has decided to type.
        autoFocus={!IS_MOBILE}
        ref={composer}
        above={
          showChips && (
            <div className="chip-row" data-leaving={chipsLeaving || undefined}>
              {pins.map((s, i) => (
                <Chip
                  key={s}
                  text={s}
                  index={i}
                  pinned
                  canPin
                  onSend={() => ask(s)}
                  onToggle={() => toggle(s)}
                />
              ))}
              {suggested.map((s, i) => (
                <Chip
                  key={s}
                  text={s}
                  index={pins.length + i}
                  pinned={false}
                  canPin={!full}
                  onSend={() => ask(s)}
                  onToggle={() => toggle(s)}
                />
              ))}
            </div>
          )
        }
        note={
          <div style={{ display: "flex", alignItems: "baseline", gap: 14 }}>
            {/* Where the question goes used to be here, one sentence per
                provider. It is a setup-time fact, and Settings is where it is
                now decided and said; under the box, every turn, the thing worth
                repeating is that the answer might be wrong. */}
            <span style={{ flex: 1, minWidth: 0 }}>
              AI can be inaccurate — take it with a grain of salt.
            </span>
            {/* Down here rather than in a header, because a docked composer is
                where your attention already is — and a header would scroll away
                from a conversation long enough to want its history. */}
            <button
              type="button"
              className="quiet"
              style={{ flex: "none", fontSize: "var(--fs-caption)" }}
              onClick={() => setRecents(true)}
            >
              Recents
            </button>
            {!empty && (
              <button
                type="button"
                className="quiet"
                style={{
                  flex: "none",
                  display: "inline-flex",
                  alignItems: "center",
                  gap: 5,
                  fontSize: "var(--fs-caption)",
                }}
                disabled={chat.busy}
                onClick={() => {
                  chat.reset();
                  // Back to the empty screen, so back to where the caret was
                  // when you arrived at it. Not done from the drawer's own New,
                  // where the drawer returns focus to whatever opened it as it
                  // closes and would take it straight back off the box.
                  composer.current?.focus();
                }}
              >
                <NewIcon size={12} style={{ flex: "none" }} aria-hidden />
                New
              </button>
            )}
          </div>
        }
      />

      {recents && (
        <Drawer title="Earlier conversations" onClose={() => setRecents(false)}>
          <Recents
            openId={chat.sessionId}
            onOpen={(id) => void chat.load(id)}
            onNew={chat.reset}
            onClose={() => setRecents(false)}
          />
        </Drawer>
      )}
    </>
  );
}

/**
 * What the screen says before it has been asked anything.
 *
 * It is the same `PageHeader` every other screen opens with, at the top of the
 * column where every other screen puts it. It used to sit low, just above the
 * composer, on the reasoning that the two read as one thing — but that made Ask
 * the one screen whose title wasn't where the eye goes first, and a person
 * arriving from Today had to find it. Consistency wins: the greeting is a page
 * title like any other, and the composer is chrome docked to the bottom
 * regardless of what is written at the top.
 *
 * No action beside it. Recents and New live under the box, where the note there
 * explains why.
 *
 * Its own component because it is drawn twice for a fraction of a second — once
 * in the flow, and once as the fixed copy that fades as the thread takes its
 * place. Two copies of the same words, so they have to come from one place.
 */
function Hero() {
  return (
    <PageHeader
      eyebrow="Ask"
      title={greeting()}
      lede="Ask about your training. It reads your cached activities, zones, cadence and recovery — only the metrics a question needs are sent."
    />
  );
}

/**
 * One suggested question, and the pin toggle beside it.
 *
 * Where the toggle went. It used to be `display: none` until React saw a
 * `mouseenter`, which had three problems and only looked like a style choice.
 * `display: none` is not in the tab order, so a keyboard could never reach it;
 * a touchscreen has no hover, so a phone could never pin anything at all; and
 * removing it from the layout changed the chip's width under the pointer, which
 * reflowed a wrapped row of chips every time you crossed one.
 *
 * So it is always in the layout, and only its opacity is conditional — on hover,
 * on focus, or on being pinned already. Where there is no hover to reveal it,
 * it simply stays visible: a control you cannot discover is not restraint. And
 * on a full shortlist the toggle isn't drawn at all rather than drawn dead,
 * because a disabled button still asks to be tried.
 */
function Chip({
  text,
  index,
  pinned,
  canPin,
  onSend,
  onToggle,
}: {
  text: string;
  index: number;
  pinned: boolean;
  canPin: boolean;
  onSend: () => void;
  onToggle: () => void;
}) {
  return (
    <span
      className="chip"
      data-pinned={pinned}
      // Staggered along the row as it appears, which is a handful of
      // milliseconds and the difference between a row arriving and a row
      // appearing. Capped, because a wrapped second line shouldn't wait.
      style={{ animationDelay: `${Math.min(index, 5) * 35}ms` }}
    >
      <button type="button" className="chip-text" onClick={onSend}>
        {text}
      </button>
      {canPin && (
        <button
          type="button"
          className="quiet chip-pin"
          onClick={onToggle}
          aria-label={pinned ? "Unpin this question" : "Pin this question"}
          title={pinned ? "Unpin this question" : `Pin this question (up to ${MAX_PINS})`}
        >
          {pinned ? <UnpinIcon size={12} /> : <PinIcon size={12} />}
        </button>
      )}
    </span>
  );
}
