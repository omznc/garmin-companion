import { useEffect, useState, type ReactNode } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link as RouterLink } from "@tanstack/react-router";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  cacheSummary,
  chatConfig,
  clearOpenrouterKey,
  garminDisconnect,
  garminStatus,
  openrouterModels,
  prepareCloudChat,
  resetChatUsage,
  setChatProvider,
  chatUsage,
  CLOUD_MODEL,
  DEFAULT_MODEL,
  setOpenrouterKey,
  themeDelete,
  themeSave,
  themesDir,
  themesOpen,
  type AiUsage,
  type ChatProvider,
  type CustomTheme,
  type ModelInfo,
} from "../lib/api";
import { ErrorNote, PageHeader, Swatch, Switch } from "../components/ui";
import {
  DeleteIcon,
  DisconnectIcon,
  EditIcon,
  ExternalIcon,
  FolderIcon,
  FullSyncIcon,
  NewIcon,
  SyncIcon,
} from "../lib/icons";
import { useContextMenu } from "../components/ContextMenu";
import { FIELDS, blankTheme } from "../lib/customTheme";
import { UpdateCheck } from "../components/UpdateCheck";
import { since } from "../lib/format";
import { useTheme } from "../lib/useTheme";
import { useTypeface } from "../lib/useTypeface";
import { runSync } from "../lib/syncProgress";
import { IS_MOBILE, STORE } from "../lib/platform";
import { dynamicTheme } from "../lib/dynamic";
import {
  BUILT_IN,
  DYNAMIC,
  PRESETS,
  builtIn,
  customPalette,
  previewTheme,
  refreshCustomThemes,
  repaint,
  setPalette,
  type Palette,
  type Theme,
} from "../lib/theme";

/**
 * Every section opens the same way and sits the same distance from the one
 * above it. This used to be five hand-set gaps — 44, 44, 64, 64 — with the
 * eyebrow's own bottom margin varying underneath them, so sections that
 * belonged to the same level read as if they were nested at different ones.
 */
function Section({
  title,
  lede,
  children,
  first = false,
}: {
  title: string;
  lede?: ReactNode;
  children: ReactNode;
  first?: boolean;
}) {
  return (
    <section style={{ marginTop: first ? 0 : 62 }}>
      <div className="eyebrow" style={{ marginBottom: lede ? 12 : 20 }}>
        {title}
      </div>
      {lede && (
        <p className="lede" style={{ maxWidth: "62ch", margin: "0 0 20px" }}>
          {lede}
        </p>
      )}
      {children}
    </section>
  );
}

/**
 * A setting on the left, the thing that changes it on the right. Geometry is
 * copied from `Switch` deliberately — a toggle and a set of words have to line
 * up as one list, or a section of two settings looks like two sections.
 */
function Setting({
  label,
  note,
  control,
}: {
  label: ReactNode;
  note?: ReactNode;
  control: ReactNode;
}) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 20,
        padding: 14,
        margin: "0 -14px",
      }}
    >
      <span style={{ flex: 1, minWidth: 0 }}>
        <span style={{ display: "block", fontSize: "var(--fs-md)" }}>{label}</span>
        {note && (
          <span
            style={{
              display: "block",
              fontSize: "var(--fs-small)",
              color: "var(--faint)",
              marginTop: 5,
              lineHeight: 1.5,
            }}
          >
            {note}
          </span>
        )}
      </span>
      <span style={{ flex: "none" }}>{control}</span>
    </div>
  );
}

/** The label above a block inside a section — a level quieter than an eyebrow. */
function Sub({ children, top = 30 }: { children: ReactNode; top?: number }) {
  return (
    <div style={{ fontSize: "var(--fs-small)", color: "var(--mut)", margin: `${top}px 0 10px` }}>
      {children}
    </div>
  );
}

/**
 * A statement of how something is set, and the way to change it.
 *
 * Used where what's behind it shouldn't be on screen unasked: a password field
 * you set once a year, and a model list that is a network request. At rest this
 * is one line of fact, which is all anyone opening Settings came to read.
 */
function Disclosure({
  fact,
  action,
  onOpen,
}: {
  fact: ReactNode;
  action: string;
  onOpen: () => void;
}) {
  return (
    <div className="row-static" style={{ justifyContent: "space-between", gap: 20 }}>
      <span style={{ minWidth: 0, color: "var(--mut)" }}>{fact}</span>
      <button
        className="underlined"
        style={{ flex: "none", fontSize: "var(--fs-small)" }}
        onClick={onOpen}
      >
        {action}
      </button>
    </div>
  );
}

export function Settings() {
  const qc = useQueryClient();
  // `setPalette` is imported rather than taken from the hook — it's the same
  // module-level function, and destructuring it here would shadow the import
  // that `ThemeEditor` below uses.
  const { theme, setTheme, palette, preset, custom, customs, paletteName } = useTheme();

  // The theme being edited, if any. Up here rather than inside the editor
  // because two things start an edit — the button under the list, and the
  // right-click menu on a row in it.
  const [draft, setDraft] = useState<CustomTheme | null>(null);
  const { typeface, setTypeface } = useTypeface();

  const status = useQuery({ queryKey: ["garminStatus"], queryFn: garminStatus });
  const cache = useQuery({ queryKey: ["cacheSummary"], queryFn: cacheSummary });
  const chat = useQuery({ queryKey: ["chatConfig"], queryFn: chatConfig });

  const [syncError, setSyncError] = useState<string | null>(null);
  const sync = useMutation({
    mutationFn: (full: boolean) => runSync(full ? 365 : 30, full),
    onMutate: () => setSyncError(null),
    onSuccess: () => qc.invalidateQueries(),
    onError: (e) => setSyncError(e instanceof Error ? e.message : String(e)),
  });

  const disconnect = useMutation({
    mutationFn: garminDisconnect,
    onSuccess: () => qc.invalidateQueries(),
  });

  return (
    <div className="screen">
      <PageHeader
        eyebrow="This machine"
        title="Settings"
        lede="Your Garmin history lives here, in a SQLite file you can delete. The only thing that leaves is a question and the handful of rows needed to answer it."
        space={52}
      />

      {/* ---------------------------------------------------------- garmin */}
      <Section title="Garmin" first>
        <div
          style={{
            display: "flex",
            alignItems: "baseline",
            gap: 12,
            fontSize: "var(--fs-md)",
            lineHeight: 1.6,
          }}
        >
          <span
            style={{
              width: 5,
              height: 5,
              borderRadius: "50%",
              background: status.data?.connected ? "var(--acc)" : "var(--line)",
              flex: "none",
              transform: "translateY(-3px)",
            }}
          />
          <span>
            {status.data?.connected
              ? `Connected. Tokens are held in ${STORE}, never in the database.`
              : "Not connected."}
            {cache.data && (
              <span style={{ color: "var(--mut)" }}>
                {" "}
                {cache.data.activities.toLocaleString()} activities cached
                {cache.data.lastSync
                  ? `, last synced ${since(cache.data.lastSync)}`
                  : ", never synced"}
                .
              </span>
            )}
          </span>
        </div>

        {cache.data?.path && (
          <div
            className="mono selectable"
            style={{ fontSize: "var(--fs-caption)", color: "var(--faint)", marginTop: 10 }}
          >
            {cache.data.path}
          </div>
        )}

        <div
          style={{
            display: "flex",
            alignItems: "baseline",
            gap: 26,
            marginTop: 24,
            fontSize: "var(--fs-small)",
          }}
        >
          <button
            className="quiet action"
            onClick={() => sync.mutate(false)}
            disabled={sync.isPending}
          >
            <SyncIcon size={13} className={sync.isPending ? "spin" : undefined} aria-hidden />
            {sync.isPending ? "Syncing…" : "Sync recent"}
          </button>
          <button
            className="quiet action"
            onClick={() => sync.mutate(true)}
            disabled={sync.isPending}
          >
            <FullSyncIcon size={13} aria-hidden />
            Full re-sync
          </button>
          {/* Pushed to the far end: it undoes the other two rather than sitting
              alongside them, and it's the one click here you can't take back. */}
          <button
            className="quiet danger action"
            style={{ marginLeft: "auto" }}
            onClick={() => disconnect.mutate()}
          >
            <DisconnectIcon size={13} aria-hidden />
            Disconnect
          </button>
        </div>

        {syncError && <ErrorNote error={syncError} />}
        {sync.data && (
          <div style={{ fontSize: "var(--fs-small)", color: "var(--mut)", marginTop: 14 }}>
            {sync.data.activitiesSeen} seen, {sync.data.activitiesWritten} new or updated,{" "}
            {sync.data.daysWritten} days of wellness data.
            {sync.data.warnings.length > 0 && (
              <div style={{ color: "var(--faint)", marginTop: 6 }}>
                {sync.data.warnings.length} warning
                {sync.data.warnings.length === 1 ? "" : "s"}: {sync.data.warnings[0]}
              </div>
            )}
          </div>
        )}
      </Section>

      {/* ----------------------------------------------------------- model */}
      <Section title="Model">
        <ModelSettings
          current={chat.data}
          onChanged={() => qc.invalidateQueries({ queryKey: ["chatConfig"] })}
        />
      </Section>

      {/* ----------------------------------------------------------- usage */}
      {/* The built-in coach is paid for by this project, so its price is not
          the user's problem and the section drops the money framing entirely
          — it counts requests and tokens instead. */}
      <Section
        title={chat.data?.provider === "cloud" ? "What it has used" : "What it has cost"}
        lede="One answer is several requests — the model asks for data, reads it, and often asks again. These are the totals across all of them."
      >
        <UsagePanel />
      </Section>

      {/* ------------------------------------------------------ appearance */}
      <Section title="Appearance">
        <Setting
          label="Theme"
          note={
            // Material You is the exception the sentence below can't cover:
            // it has both appearances, so the mode still picks between them
            // and is not set aside at all.
            palette === DYNAMIC ? (
              "Material You comes in both, so this still chooses which one you get."
            ) : paletteName ? (
              <>
                Set aside while {paletteName} is on — a palette is always{" "}
                {(preset ?? custom)?.appearance}. Choose Default below to hand it back.
              </>
            ) : (
              `Match system follows whatever your ${IS_MOBILE ? "phone" : "desktop"} is set to.`
            )
          }
          control={
            <ThemeChoice
              theme={theme}
              onChange={setTheme}
              overridden={!!paletteName && palette !== DYNAMIC}
            />
          }
        />

        <Sub top={26}>Palette</Sub>
        <PaletteChoice
          theme={theme}
          palette={palette}
          customs={customs}
          onChange={setPalette}
          onEdit={setDraft}
        />

        <ThemeMaker draft={draft} onDraft={setDraft} hasCustoms={customs.length > 0} />

        <Switch
          on={typeface === "sans"}
          onChange={(on) => setTypeface(on ? "sans" : "serif")}
          label="Remove hopes and dreams"
          note="Drops the serif display face if you find it hard to read."
        />
      </Section>

      {/* ------------------------------------------------------------ data */}
      <Section title="What leaves this machine">
        <div
          style={{
            fontSize: "var(--fs-md)",
            lineHeight: 1.7,
            color: "var(--mut)",
            maxWidth: "62ch",
            textWrap: "pretty",
          }}
        >
          <p style={{ margin: "0 0 12px" }}>
            Garmin requests go straight from this app to Garmin, using tokens in {STORE} — never
            through a server of ours.
          </p>
          <p style={{ margin: 0 }}>
            Chat sends your question plus whatever a tool returned for it — a handful of summary
            rows, never the whole database and never GPS traces.{" "}
            {/* Named rather than generalised. "A server" is the sentence that
                lets someone assume it isn't ours, and on the default provider
                it is. */}
            {chat.data?.provider === "ollama"
              ? "With Ollama selected, not even that leaves the machine."
              : chat.data?.provider === "cloud"
                ? "The built-in coach routes that through a small server this project runs, which forwards it to a hosted model and keeps none of it. OpenRouter with your own key, or a local Ollama, both skip that server."
                : "With your own OpenRouter key, that goes straight to OpenRouter. A local Ollama would keep it on this machine."}
          </p>
        </div>
      </Section>

      {/* --------------------------------------------------------- version */}
      {/* Every platform, including Android — which used to be excluded on the
          belief that an app can't replace its own package. It can; it just
          can't do it quietly. See `lib/updater.ts` for what differs. */}
      <Section title="Version">
        <UpdateCheck />
      </Section>

      {/* ----------------------------------------------------------- about */}
      <Section title="About">
        <div
          className="selectable"
          style={{
            fontSize: "var(--fs-md)",
            lineHeight: 1.7,
            color: "var(--mut)",
            maxWidth: "62ch",
            textWrap: "pretty",
          }}
        >
          <p style={{ margin: "0 0 12px" }}>
            I was always really into data, and Garmin is a data powerhouse. I used to export it and
            then look at it in excel, nowadays I just point the claude to it, but I wanted something
            easier for non-technical people.
          </p>
          <p style={{ margin: "0 0 12px" }}>
            This is it, you're looking at it. A cross-platform app where you just log in and that's
            it, full free access to AI analysis of your Garmin data. Want more privacy? Use a local
            model. It's all open source, and fully up to you,
          </p>
          <p style={{ margin: 0 }}>
            — <Link href="https://omarzunic.com">Omar Žunić</Link>
          </p>
        </div>
      </Section>
    </div>
  );
}

/**
 * A link out of the app.
 *
 * A bare anchor would navigate the webview itself — the app replaced by a
 * website, with no back button to leave it by — so the click is handed to the
 * opener plugin and the real browser. `href` stays on the element regardless,
 * so the URL is still visible to a screen reader and to right-click.
 */
function Link({ href, children }: { href: string; children: ReactNode }) {
  return (
    <a
      href={href}
      onClick={(e) => {
        e.preventDefault();
        void openUrl(href);
      }}
    >
      {children}
      <ExternalIcon size={12} style={{ verticalAlign: -1, marginLeft: 4 }} aria-hidden />
    </a>
  );
}

/* ------------------------------------------------------------- appearance --- */

/**
 * Light, dark, match system — and what that row becomes once it has stopped
 * deciding anything.
 *
 * The three stay on screen while a palette overrides them rather than being
 * hidden or swapped for a sentence, because what's stored under a palette is
 * what you get back when you leave it, and being able to read that off the row
 * is the point of keeping it. `disabled` rather than a dimmed lookalike: the
 * control genuinely cannot be operated, and saying so is what gets the state
 * announced to a screen reader instead of only shown.
 *
 * The marker under the stored choice drops from a solid accent rule to a
 * hairline — the same fact, at the volume of something not currently in force.
 */
function ThemeChoice({
  theme,
  onChange,
  overridden,
}: {
  theme: Theme;
  onChange: (t: Theme) => void;
  overridden: boolean;
}) {
  return (
    <div style={{ display: "flex", gap: 18, fontSize: "var(--fs-base)" }}>
      {(["light", "dark", "system"] as Theme[]).map((t) => {
        const on = theme === t;
        return (
          <button
            key={t}
            onClick={() => onChange(t)}
            disabled={overridden}
            style={{
              cursor: overridden ? "default" : "pointer",
              color: overridden ? "var(--faint)" : on ? "var(--fg)" : "var(--mut)",
              borderBottom: `1px solid ${
                !on ? "transparent" : overridden ? "var(--line)" : "var(--acc)"
              }`,
              paddingBottom: 3,
              textTransform: "capitalize",
              transition: "color var(--dur-base), border-color var(--dur-base)",
            }}
          >
            {t === "system" ? "Match system" : t}
          </button>
        );
      })}
    </div>
  );
}

/**
 * Default, then the premade palettes.
 *
 * The swatch is the row's real content — "Moss" is a label for something you
 * can only judge by looking at it. It draws itself by wearing the palette's own
 * `data-palette` and reading `--bg`, `--acc` and `--fg` back out, so this list
 * never learns a colour: a new palette is a block in `styles.css` and an entry
 * in `lib/theme.ts`, and this picks it up with no third edit.
 *
 * Default is the one row that has to be told which handle to wear, because what
 * it previews is whatever the theme above currently resolves to.
 */
/** How many palettes the list shows before it stops and offers the rest. */
const PALETTES_SHOWN = 5;

function PaletteChoice({
  theme,
  palette,
  customs,
  onChange,
  onEdit,
}: {
  theme: Theme;
  palette: Palette;
  customs: CustomTheme[];
  onChange: (p: Palette) => void;
  onEdit: (t: CustomTheme) => void;
}) {
  const [all, setAll] = useState(false);
  const menu = useContextMenu();

  const remove = useMutation({
    mutationFn: themeDelete,
    onSuccess: () => refreshCustomThemes(),
  });

  // Null on a desktop, and on an Android older than 12 — there is no wallpaper
  // palette to read, so there is no row to offer. Built for the mode rather
  // than for a fixed appearance: the swatch has to preview what picking it
  // would actually give you, and that depends on which half of the wallpaper's
  // colours the mode is currently asking for.
  const dynamicSwatch = dynamicTheme(builtIn(theme));

  /**
   * Default, Material You where there is one, the shipped palettes, then yours.
   *
   * Yours last because they're additions to a list that already existed, and
   * because the cut below can't hide the one that matters: a theme is selected
   * the moment it's made — by the editor, or by the model calling `use_theme`
   * — and the selected row is always drawn.
   */
  const rows: Array<{
    id: Palette;
    name: string;
    note: string;
    swatch: string | CustomTheme;
    appearance: string;
    /** Present on the rows that are a file, which is what can be edited. */
    theme?: CustomTheme;
  }> = [
    {
      id: null,
      name: "Default",
      note: "The app's own — warm paper, ember",
      swatch: BUILT_IN[builtIn(theme)],
      appearance: builtIn(theme),
    },
    // Second, and above the cut, on the phones that have it. It is the default
    // there, so the row it is a row of has to be the one you can find without
    // expanding a list — this is the way back to the palette the app was
    // designed in, and it should not be the thing you have to go looking for.
    //
    // Mode-aware like Default and unlike everything after it: the swatch and
    // the appearance both follow the mode, because the wallpaper gives both.
    ...(dynamicSwatch
      ? [
          {
            id: DYNAMIC,
            name: "Material You",
            note: "From your wallpaper",
            swatch: dynamicSwatch,
            appearance: builtIn(theme),
          },
        ]
      : []),
    ...PRESETS.map((p) => ({
      id: p.id,
      name: p.name,
      note: p.note,
      swatch: p.id,
      appearance: p.appearance,
    })),
    ...customs.map((c) => ({
      id: customPalette(c.slug),
      name: c.name,
      note: c.note,
      swatch: c,
      appearance: c.appearance,
      theme: c,
    })),
  ];

  // The one in force is always drawn, even from beyond the cut. A list that
  // hides what you're currently looking at is a list that has to be expanded
  // before it can be read.
  const hidden = all ? [] : rows.slice(PALETTES_SHOWN).filter((r) => r.id !== palette);
  const shown = rows.filter((r) => !hidden.includes(r));

  return (
    <>
      {menu.menu}
      {shown.map((r) => {
        const on = palette === r.id;
        return (
          <button
            key={r.id ?? "default"}
            className="row"
            onClick={() => onChange(r.id)}
            // Only the rows backed by a file have anything to offer. The
            // shipped four aren't yours to edit or throw away, so they keep the
            // webview's menu suppressed and open nothing.
            onContextMenu={(e) =>
              r.theme &&
              menu.open(e, [
                {
                  label: "Edit",
                  icon: EditIcon,
                  onSelect: () => {
                    onChange(r.id);
                    onEdit(r.theme!);
                  },
                },
                {
                  label: `Delete ${r.name}`,
                  icon: DeleteIcon,
                  divide: true,
                  onSelect: () => remove.mutate(r.theme!.slug),
                },
              ])
            }
          >
            <span
              style={{
                width: 5,
                height: 5,
                borderRadius: "50%",
                flex: "none",
                background: on ? "var(--acc)" : "var(--line)",
                transform: "translateY(-3px)",
              }}
            />
            <span style={{ flex: 1, minWidth: 0, color: on ? "var(--fg)" : "var(--mut)" }}>
              {r.name}
              {r.note && (
                <span
                  style={{
                    color: "var(--faint)",
                    fontSize: "var(--fs-small)",
                    marginLeft: 10,
                  }}
                >
                  {r.note}
                </span>
              )}
            </span>
            <Swatch of={r.swatch} />
            <span
              style={{
                fontSize: "var(--fs-caption)",
                color: "var(--faint)",
                flex: "none",
                width: 34,
                textAlign: "right",
                textTransform: "capitalize",
              }}
            >
              {r.appearance}
            </span>
          </button>
        );
      })}

      {(hidden.length > 0 || all) && (
        <button
          className="underlined"
          style={{ fontSize: "var(--fs-small)", marginTop: 14 }}
          onClick={() => setAll((v) => !v)}
        >
          {all ? "Show fewer" : `View ${hidden.length} more`}
        </button>
      )}
    </>
  );
}

/* ------------------------------------------------------------ theme maker --- */

/**
 * The two ways to get a theme that isn't one of the six above: describe one, or
 * mix one.
 *
 * Describing it is listed first because it is the one that works from "I want
 * something like a forest at dusk", which is how anyone actually arrives here.
 * The editor is for the second pass — the accent is a shade too red — and for
 * people who would rather not ask.
 */
function ThemeMaker({
  draft,
  onDraft,
  hasCustoms,
}: {
  draft: CustomTheme | null;
  onDraft: (t: CustomTheme | null) => void;
  hasCustoms: boolean;
}) {
  const dir = useQuery({ queryKey: ["themesDir"], queryFn: themesDir });

  if (draft) {
    return <ThemeEditor draft={draft} onDraft={onDraft} onClose={() => onDraft(null)} />;
  }

  return (
    <div style={{ marginTop: 30 }}>
      <p className="lede" style={{ maxWidth: "62ch", margin: "0 0 16px" }}>
        Describe one in <RouterLink to="/ask">Ask</RouterLink> — "a dark theme like a forest at
        dusk" — and it will mix the colours, save it here and put it on. Or set the seven yourself.
        {hasCustoms && " Right-click one of yours above to edit or delete it."}
      </p>

      <div
        style={{
          display: "flex",
          alignItems: "baseline",
          gap: 26,
          fontSize: "var(--fs-small)",
        }}
      >
        <button className="quiet action" onClick={() => onDraft(blankTheme("dark"))}>
          <NewIcon size={13} aria-hidden />
          New theme
        </button>
        <button
          className="quiet action"
          style={{ marginLeft: "auto" }}
          onClick={() => void themesOpen()}
        >
          <FolderIcon size={13} aria-hidden />
          Open folder
        </button>
      </div>

      {dir.data && (
        <div
          className="mono selectable"
          style={{
            fontSize: "var(--fs-caption)",
            color: "var(--faint)",
            marginTop: 12,
          }}
        >
          {dir.data}
        </div>
      )}
    </div>
  );
}

/** `#abc` written out, which is the only form a colour input accepts. */
function expand(hex: string): string {
  const b = hex.replace("#", "");
  return b.length === 3 ? `#${[...b].map((c) => c + c).join("")}` : `#${b}`;
}

/**
 * Seven colours, a name, and which of the two it is.
 *
 * Every keystroke previews on the whole window rather than into a mock panel.
 * A theme is the app — the only honest preview of one is the app wearing it,
 * and a swatch grid can't show you that your `faint` disappeared against the
 * page. Nothing is written until Save, and closing puts back what was selected.
 */
function ThemeEditor({
  draft,
  onDraft,
  onClose,
}: {
  draft: CustomTheme;
  onDraft: (t: CustomTheme) => void;
  onClose: () => void;
}) {
  const [error, setError] = useState<string | null>(null);

  // Preview on mount and on every edit; put the real palette back on the way
  // out, however the editor closes.
  useEffect(() => {
    previewTheme(draft);
  }, [draft]);
  useEffect(() => repaint, []);

  const edit = (patch: Partial<CustomTheme>) => onDraft({ ...draft, ...patch });
  const color = (key: keyof CustomTheme["colors"], value: string) =>
    onDraft({ ...draft, colors: { ...draft.colors, [key]: value } });

  const save = useMutation({
    mutationFn: () => themeSave(draft),
    onMutate: () => setError(null),
    onSuccess: async (saved) => {
      await refreshCustomThemes();
      // Selecting it is the point of having made it, and it's already what the
      // window looks like — leaving the preview showing a theme that isn't
      // selected would undo itself the moment anything repainted.
      setPalette(customPalette(saved.slug));
      onClose();
    },
    onError: (e) => setError(e instanceof Error ? e.message : String(e)),
  });

  const remove = useMutation({
    mutationFn: () => themeDelete(draft.slug),
    onSuccess: async () => {
      await refreshCustomThemes();
      onClose();
    },
    onError: (e) => setError(e instanceof Error ? e.message : String(e)),
  });

  return (
    <div style={{ marginTop: 30 }}>
      <div style={{ display: "flex", gap: 14, marginBottom: 6 }}>
        <input
          className="input input-lg"
          value={draft.name}
          onChange={(e) => edit({ name: e.target.value })}
          placeholder="Name it"
          style={{ flex: "1 1 40%" }}
          autoFocus
        />
        <input
          className="input input-lg"
          value={draft.note}
          onChange={(e) => edit({ note: e.target.value })}
          placeholder="A few words about it"
          style={{ flex: "1 1 60%" }}
        />
      </div>

      <Setting
        label="Appearance"
        note="Fixed. This is what the theme is, not what it follows."
        control={
          <div style={{ display: "flex", gap: 18, fontSize: "var(--fs-base)" }}>
            {(["light", "dark"] as const).map((a) => (
              <button
                key={a}
                // Only the appearance. The colours are the author's, and
                // swapping them out from under a half-finished theme because
                // the switch moved would be the editor overruling them.
                onClick={() => edit({ appearance: a })}
                style={{
                  cursor: "pointer",
                  color: draft.appearance === a ? "var(--fg)" : "var(--mut)",
                  borderBottom: `1px solid ${draft.appearance === a ? "var(--acc)" : "transparent"}`,
                  paddingBottom: 3,
                  textTransform: "capitalize",
                }}
              >
                {a}
              </button>
            ))}
          </div>
        }
      />

      {FIELDS.map((f) => (
        <div key={f.key} className="row" style={{ cursor: "default" }}>
          <span style={{ flex: 1, minWidth: 0 }}>
            {f.label}
            <span
              style={{
                color: "var(--faint)",
                fontSize: "var(--fs-small)",
                marginLeft: 10,
              }}
            >
              {f.note}
            </span>
          </span>
          <input
            className="mono input-hex"
            value={draft.colors[f.key]}
            onChange={(e) => color(f.key, e.target.value)}
            spellCheck={false}
            aria-label={`${f.label} colour, as hex`}
          />
          <input
            type="color"
            className="well"
            value={expand(draft.colors[f.key])}
            onChange={(e) => color(f.key, e.target.value)}
            aria-label={`${f.label} colour`}
          />
        </div>
      ))}

      <div
        style={{
          display: "flex",
          alignItems: "baseline",
          gap: 22,
          marginTop: 22,
          fontSize: "var(--fs-small)",
        }}
      >
        <span style={{ flex: 1, minWidth: 0, color: "var(--faint)" }}>
          You're looking at it. Nothing is saved until you say so.
        </span>
        {draft.slug && (
          <button className="quiet danger" onClick={() => remove.mutate()}>
            Delete
          </button>
        )}
        <button className="quiet" onClick={onClose}>
          Cancel
        </button>
        <button
          className="underlined"
          style={{ fontSize: "var(--fs-small)" }}
          onClick={() => save.mutate()}
          disabled={!draft.name.trim() || save.isPending}
        >
          {save.isPending ? "Saving…" : "Save theme"}
        </button>
      </div>

      {error && <ErrorNote error={error} />}
    </div>
  );
}

/* ------------------------------------------------------------------ usage --- */

/** Tokens, at the scale they actually accumulate in. */
function tokenCount(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${Math.round(n / 1_000)}k`;
  return n.toLocaleString();
}

/** One number, big, with its name above it and its caveat below. */
function Figure({ label, value, note }: { label: string; value: string; note?: string }) {
  return (
    <div style={{ minWidth: 96 }}>
      <div style={{ fontSize: "var(--fs-caption)", color: "var(--mut)", marginBottom: 6 }}>
        {label}
      </div>
      <div style={{ fontSize: 22, lineHeight: 1.1 }}>{value}</div>
      {note && (
        <div style={{ fontSize: "var(--fs-micro)", color: "var(--faint)", marginTop: 6 }}>
          {note}
        </div>
      )}
    </div>
  );
}

/** One name per provider, so the totals and the picker never disagree. */
const PROVIDER_NAME: Record<ChatProvider, string> = {
  cloud: "Built-in coach",
  openrouter: "OpenRouter",
  ollama: "Ollama",
};

/** Four decimals under a dollar, because most questions cost less than a cent. */
const usd = (n: number) => `$${n.toFixed(n < 1 ? 4 : 2)}`;

/**
 * What the providers you aren't using now ran up while you were.
 *
 * Here because switching provider doesn't unspend the money. The totals are
 * counted per provider so a dollar on your own OpenRouter key can't be read
 * back as a dollar on the built-in coach — but that split would hide the dollar
 * entirely if the panel only ever showed the current one.
 *
 * The built-in coach is billed to this project rather than to whoever is
 * reading, so it reports requests here the way Ollama does — the money it cost
 * is real, but it isn't theirs to see.
 */
function OtherTotals({ items }: { items: AiUsage[] }) {
  if (items.length === 0) return null;
  return (
    <div
      style={{
        marginTop: 22,
        fontSize: "var(--fs-small)",
        color: "var(--faint)",
        lineHeight: 1.5,
      }}
    >
      Counted separately, from providers you aren't using now:{" "}
      {items
        .map((o) => {
          if (o.provider !== "cloud" && o.costUsd > 0) {
            return `${PROVIDER_NAME[o.provider]} ${usd(o.costUsd)}`;
          }
          const n = `${o.requests.toLocaleString()} request${o.requests === 1 ? "" : "s"}`;
          return `${PROVIDER_NAME[o.provider]} ${
            o.provider === "cloud" ? n : `${n}, nothing billed`
          }`;
        })
        .join(" · ")}
      . Switch to one to see or reset its totals.
    </div>
  );
}

/**
 * What the model has spent.
 *
 * Its own query rather than a field on the chat config, because that one probes
 * Ollama over the network and this is a read of a few rows. Nothing here polls —
 * the totals are refetched when this screen mounts, which is when someone has
 * come to look at them.
 */
function UsagePanel() {
  const qc = useQueryClient();
  // The config is read for the query key, not for the numbers: totals are kept
  // per provider, so switching provider asks a different question and the old
  // answer must not be left on screen under the new name. The query is already
  // in the cache because the panel above it uses the same key.
  const config = useQuery({ queryKey: ["chatConfig"], queryFn: chatConfig });
  const usage = useQuery({
    queryKey: ["chatUsage", config.data?.provider ?? null],
    queryFn: chatUsage,
  });
  const reset = useMutation({
    mutationFn: () => resetChatUsage(),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["chatUsage"] }),
  });

  const u = usage.data?.current;
  const others = usage.data?.others ?? [];
  if (!u || u.requests === 0) {
    return (
      <div style={{ fontSize: "var(--fs-small)", color: "var(--faint)" }}>
        {u
          ? u.provider === "cloud"
            ? `Nothing asked yet of the ${PROVIDER_NAME[u.provider].toLowerCase()}.`
            : `Nothing spent yet on ${PROVIDER_NAME[u.provider]}.`
          : "Nothing spent yet."}{" "}
        Ask a question and the totals start here.
        <OtherTotals items={others} />
      </div>
    );
  }

  // A hosted provider reports a price; Ollama reports nothing because there
  // isn't one. Zero is therefore ambiguous, and saying so beats printing $0.00.
  // The built-in coach does report one, but it's this project's bill, not the
  // reader's, so the figure is left out rather than shown as somebody else's
  // money — the requests and tokens beside it still say how much was used.
  const priced = u.provider !== "cloud" && u.costUsd > 0;
  const cachedPct = u.promptTokens ? Math.round((u.cachedTokens / u.promptTokens) * 100) : 0;

  return (
    <div>
      <div style={{ display: "flex", flexWrap: "wrap", gap: "28px 48px" }}>
        {u.provider !== "cloud" && (
          <Figure
            label="Cost"
            value={priced ? usd(u.costUsd) : "—"}
            note={priced ? "as OpenRouter billed it" : "nothing billed"}
          />
        )}
        <Figure label="Requests" value={u.requests.toLocaleString()} note="one answer is several" />
        <Figure
          label="Sent"
          value={tokenCount(u.promptTokens)}
          note={u.cachedTokens > 0 ? `${cachedPct}% read from cache` : "none cached"}
        />
        <Figure label="Received" value={tokenCount(u.completionTokens)} />
      </div>

      <div
        style={{
          display: "flex",
          alignItems: "baseline",
          gap: 26,
          marginTop: 26,
          fontSize: "var(--fs-small)",
          color: "var(--faint)",
        }}
      >
        <span>
          {PROVIDER_NAME[u.provider]},{" "}
          {u.since ? `counting since ${since(u.since)}.` : "counting since the first request."}
        </span>
        <button
          className="quiet action"
          style={{ marginLeft: "auto" }}
          onClick={() => reset.mutate()}
          disabled={reset.isPending}
        >
          Reset
        </button>
      </div>

      <OtherTotals items={others} />

      {usage.error && <ErrorNote error="Couldn't read the usage totals." />}
    </div>
  );
}

/* ------------------------------------------------------------------ model --- */

function ModelSettings({
  current,
  onChanged,
}: {
  current: Awaited<ReturnType<typeof chatConfig>> | undefined;
  onChanged: () => void;
}) {
  const [key, setKey] = useState("");
  const [error, setError] = useState<string | null>(null);
  /** Which of the two panels is open, if either. Neither, on arrival. */
  const [open, setOpen] = useState<"key" | "model" | null>(null);
  /**
   * The model search, owned up here rather than inside the picker, because it
   * is what decides whether the catalogue gets fetched at all.
   */
  const [q, setQ] = useState("");

  const provider = current?.provider ?? null;

  const models = useQuery({
    queryKey: ["openrouterModels"],
    queryFn: openrouterModels,
    /**
     * Three hundred-odd models, fetched over the network, to fill a list nobody
     * asked to see: opening Settings used to pay for that every time. So the
     * request waits until someone opens the picker *and* starts typing, which
     * is the first moment the answer is worth anything. Once it has run the
     * result is cached for the hour, so reopening the picker is free — a
     * disabled query still reads what's already in the cache.
     */
    enabled: provider === "openrouter" && open === "model" && q.trim().length > 0,
    staleTime: 60 * 60_000,
    retry: false,
  });

  const closePanels = () => {
    setOpen(null);
    setKey("");
    setError(null);
  };

  const save = useMutation({
    // One mutation for both "switch provider" and "pick a model", because
    // choosing either without the other is never a state worth saving.
    mutationFn: async ({ p, model }: { p: ChatProvider; model?: ModelInfo | string }) => {
      // Cloud takes its own and nothing else — carrying the outgoing provider's
      // model across would hand the proxy an id it rejects. The backend pins it
      // too; this is so the screen never briefly claims otherwise.
      const chosen =
        p === "cloud"
          ? CLOUD_MODEL
          : ((typeof model === "string" ? model : model?.id) ??
            current?.model ??
            defaultModel(p, current));
      if (!chosen) throw new Error("Pick a model first.");
      await setChatProvider(p, chosen, typeof model === "object" ? model.structured : undefined);
      // Same as at the end of setup: get the id now so the first question after
      // switching isn't the one that pays for it. Ignored on failure — the
      // switch itself succeeded, and saying otherwise would be wrong.
      if (p === "cloud") await prepareCloudChat().catch(() => {});
    },
    onSuccess: () => {
      // Picking answers the question the panel was opened to ask, so it closes.
      closePanels();
      setQ("");
      onChanged();
    },
    onError: (e) => setError(e instanceof Error ? e.message : String(e)),
  });

  // The key used to ride along with whatever model was picked next, which meant
  // a key on its own couldn't be saved at all. It has its own button now, so it
  // has its own write.
  const saveKey = useMutation({
    mutationFn: async () => {
      await setOpenrouterKey(key.trim());
      // A key with nothing selected leaves chat unconfigured. The default is a
      // better landing place than that, and the picker below changes it.
      if (!current?.model) await setChatProvider("openrouter", DEFAULT_MODEL);
    },
    onSuccess: () => {
      closePanels();
      onChanged();
    },
    onError: (e) => setError(e instanceof Error ? e.message : String(e)),
  });

  const forget = useMutation({
    mutationFn: clearOpenrouterKey,
    onSuccess: () => {
      closePanels();
      onChanged();
    },
  });

  return (
    <div>
      {/* Provider choice */}
      {(
        [
          {
            id: "cloud" as const,
            name: PROVIDER_NAME.cloud,
            note: "Nothing to set up, paid for by this project",
            tag: "Hosted",
            available: true,
          },
          {
            id: "openrouter" as const,
            name: PROVIDER_NAME.openrouter,
            note: "Any hosted model, one key",
            tag: "Hosted",
            available: true,
          },
          {
            id: "ollama" as const,
            name: PROVIDER_NAME.ollama,
            note: current?.ollamaReachable
              ? `${current.ollamaModels.length} model${current.ollamaModels.length === 1 ? "" : "s"} pulled locally`
              : "Not running on localhost:11434",
            tag: "Local",
            available: !!current?.ollamaReachable,
          },
        ] satisfies Array<{
          id: ChatProvider;
          name: string;
          note: string;
          tag: string;
          available: boolean;
        }>
      ).map((p) => (
        <button
          key={p.id}
          className="row"
          onClick={() => p.available && save.mutate({ p: p.id })}
          disabled={!p.available}
          style={{ cursor: p.available ? "pointer" : "default" }}
        >
          <span
            style={{
              width: 5,
              height: 5,
              borderRadius: "50%",
              flex: "none",
              background: provider === p.id ? "var(--acc)" : "var(--line)",
              transform: "translateY(-3px)",
            }}
          />
          <span
            style={{
              flex: 1,
              minWidth: 0,
              fontSize: "var(--fs-md)",
              color: provider === p.id ? "var(--fg)" : "var(--mut)",
            }}
          >
            {p.name}
            <span style={{ color: "var(--faint)", fontSize: "var(--fs-small)", marginLeft: 10 }}>
              {p.note}
            </span>
          </span>
          <span style={{ fontSize: "var(--fs-caption)", color: "var(--faint)", flex: "none" }}>
            {p.tag}
          </span>
        </button>
      ))}

      {/* Key */}
      {provider === "openrouter" && (
        <>
          <Sub>OpenRouter key</Sub>
          {open === "key" ? (
            <>
              <input
                type="password"
                value={key}
                onChange={(e) => setKey(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && key.trim() && saveKey.mutate()}
                placeholder={current?.hasKey ? "Type the new key" : "sk-or-v1-…"}
                className="input input-lg"
                autoFocus
              />
              <div
                style={{
                  display: "flex",
                  alignItems: "baseline",
                  gap: 20,
                  fontSize: "var(--fs-caption)",
                  color: "var(--faint)",
                  marginTop: 9,
                }}
              >
                <span style={{ flex: 1, minWidth: 0 }}>
                  Stored in your system keychain, never in the database.
                </span>
                {current?.hasKey && (
                  <button
                    className="quiet danger"
                    style={{ flex: "none" }}
                    onClick={() => forget.mutate()}
                  >
                    Forget it
                  </button>
                )}
                <button className="quiet" style={{ flex: "none" }} onClick={closePanels}>
                  Cancel
                </button>
                <button
                  className="underlined"
                  style={{ flex: "none", fontSize: "var(--fs-caption)" }}
                  onClick={() => saveKey.mutate()}
                  disabled={!key.trim() || saveKey.isPending}
                >
                  {saveKey.isPending ? "Saving…" : "Save key"}
                </button>
              </div>
            </>
          ) : (
            <Disclosure
              fact={
                current?.hasKey
                  ? "Stored in your system keychain, never in the database."
                  : "No key yet — chat can't run without one."
              }
              action={current?.hasKey ? "Change OpenRouter key" : "Add OpenRouter key"}
              onOpen={() => {
                setError(null);
                setOpen("key");
              }}
            />
          )}

          <Sub top={34}>Model</Sub>
          {open === "model" ? (
            <ModelPicker
              models={models.data ?? []}
              q={q}
              onQ={setQ}
              loading={models.isFetching}
              failed={!!models.error}
              selected={current?.model ?? null}
              onPick={(m) => save.mutate({ p: "openrouter", model: m })}
              saving={save.isPending}
              onClose={closePanels}
            />
          ) : (
            <Disclosure
              fact={
                current?.model ? (
                  <span className="mono" style={{ fontSize: "var(--fs-small)" }}>
                    {current.model}
                  </span>
                ) : (
                  "None chosen yet."
                )
              }
              action="Change default model"
              onOpen={() => {
                setError(null);
                setOpen("model");
              }}
            />
          )}
        </>
      )}

      {/* The hosted coach has nothing to configure, which is the point of it.
          What it does have is something to disclose — this is the one provider
          where the data goes to a server the athlete didn't choose and doesn't
          run, and that belongs on screen rather than in a README. */}
      {provider === "cloud" && (
        <>
          <Sub top={34}>What this means</Sub>
          <div
            style={{
              fontSize: "var(--fs-small)",
              color: "var(--mut)",
              lineHeight: 1.65,
              maxWidth: "60ch",
            }}
          >
            Questions go to a small server this project runs, which forwards them to our own model
            and pays for the answer. It sees whatever a question needs — heart rates, sleep, weight
            — and keeps none of it. Raw GPS is never sent by any provider.
            <br />
            <br />
            It is shared, so it has daily limits, and a heavy day can run into them. Your own
            OpenRouter key or a local Ollama has no such ceiling.
            <br />
            It's free, so yeah.
          </div>

          {/* Stated, not offered. The id is what the daily limit is counted
              against, so a button that replaces it is a button that clears the
              limit — the disclosure is worth keeping, the reset isn't. */}
          <Sub top={34}>This install</Sub>
          <div className="row-static" style={{ color: "var(--mut)" }}>
            A random id the coach hands this install the first time you ask it something, held in
            your keychain so it can count requests against a shared budget. It isn't tied to your
            Garmin account or anything else.
          </div>
        </>
      )}

      {/* Ollama has no catalogue to search — it's whatever you've pulled. */}
      {provider === "ollama" && (
        <>
          <Sub top={34}>Model</Sub>
          {(current?.ollamaModels ?? []).map((m) => (
            <button
              key={m}
              className="row"
              onClick={() => save.mutate({ p: "ollama", model: m })}
              disabled={save.isPending}
            >
              <span style={{ flex: 1, color: current?.model === m ? "var(--fg)" : "var(--mut)" }}>
                {m}
              </span>
              {current?.model === m && (
                <span style={{ fontSize: "var(--fs-caption)", color: "var(--acc)", flex: "none" }}>
                  In use
                </span>
              )}
            </button>
          ))}
          <div style={{ fontSize: "var(--fs-caption)", color: "var(--faint)", marginTop: 12 }}>
            {current?.ollamaModels.length
              ? "It must support tool calls — Ollama doesn't say which do, so this list isn't filtered."
              : "No local models found."}
          </div>
        </>
      )}

      {error && <ErrorNote error={error} />}
    </div>
  );
}

/** What gets used when a provider is chosen but no model has been picked. */
function defaultModel(
  p: ChatProvider,
  current: Awaited<ReturnType<typeof chatConfig>> | undefined,
): string | null {
  if (p === "cloud") return CLOUD_MODEL;
  if (p === "openrouter") return DEFAULT_MODEL;
  return current?.ollamaModels[0] ?? null;
}

/* ----------------------------------------------------------- model picker --- */

/** How many results to draw before asking you to narrow the search. */
const SHOWN = 8;

/**
 * A search over OpenRouter's catalogue, rather than a box you type an id into.
 *
 * There are 300-odd usable models and no way to remember their ids, so the old
 * free-text field with a datalist was really a memory test with a typo waiting
 * in it. Everything listed here already passes the one hard requirement — tool
 * calls — and each row says what it costs and whether it takes a JSON schema,
 * so the choice can be made on the facts rather than on brand recognition.
 *
 * The catalogue arrives with the first keystroke rather than with the screen,
 * so this renders as an empty search box until then. `q` therefore belongs to
 * the caller, which is what decides whether the fetch happens.
 */
function ModelPicker({
  models,
  q,
  onQ,
  loading,
  failed,
  selected,
  onPick,
  saving,
  onClose,
}: {
  models: ModelInfo[];
  q: string;
  onQ: (q: string) => void;
  loading: boolean;
  failed: boolean;
  selected: string | null;
  onPick: (m: ModelInfo) => void;
  saving: boolean;
  onClose: () => void;
}) {
  const [strictOnly, setStrictOnly] = useState(false);

  const pool = strictOnly ? models.filter((m) => m.structured) : models;
  const hits = search(pool, q);
  const current = models.find((m) => m.id === selected);

  return (
    <div>
      <input
        className="input input-lg"
        value={q}
        onChange={(e) => onQ(e.target.value)}
        placeholder="Search models — name, maker, or id"
        disabled={saving}
        autoFocus
      />

      <div
        style={{
          display: "flex",
          alignItems: "baseline",
          justifyContent: "space-between",
          gap: 16,
          fontSize: "var(--fs-caption)",
          color: "var(--faint)",
          margin: "9px 0 6px",
        }}
      >
        <span style={{ minWidth: 0 }}>
          {loading
            ? "Loading OpenRouter's catalogue…"
            : models.length === 0
              ? "Type to load the catalogue. Only models that call tools are listed."
              : `${models.length} models call tools, which is what this app needs. Only these are listed.`}
        </span>
        <span style={{ display: "flex", alignItems: "baseline", gap: 18, flex: "none" }}>
          {models.length > 0 && (
            <button
              className="underlined"
              style={{ fontSize: "var(--fs-caption)" }}
              onClick={() => setStrictOnly((v) => !v)}
            >
              {strictOnly ? "Showing schema-capable only" : "Schema-capable only"}
            </button>
          )}
          <button className="quiet" onClick={onClose}>
            Cancel
          </button>
        </span>
      </div>

      {failed && (
        <ErrorNote error="Couldn't reach OpenRouter to list models. The one already chosen keeps working." />
      )}

      {/* Nothing below the box until the catalogue is in hand: an empty list and
          a "nothing matches" underneath it would both be lies before then. */}
      {models.length > 0 && (
        <>
          {/* The model in use, even when the search has scrolled it out of sight —
              otherwise picking one and searching again loses track of it. */}
          {current && !hits.some((m) => m.id === current.id) && (
            <Row model={current} selected onPick={onPick} saving={saving} />
          )}

          {hits.slice(0, SHOWN).map((m) => (
            <Row
              key={m.id}
              model={m}
              selected={m.id === selected}
              onPick={onPick}
              saving={saving}
            />
          ))}

          <div
            style={{
              fontSize: "var(--fs-caption)",
              color: "var(--faint)",
              marginTop: 12,
              lineHeight: 1.6,
            }}
          >
            {hits.length === 0
              ? "Nothing matches that."
              : hits.length > SHOWN
                ? `${hits.length - SHOWN} more — keep typing to narrow it down.`
                : "Picking one saves it straight away."}
          </div>
        </>
      )}
    </div>
  );
}

function Row({
  model,
  selected,
  onPick,
  saving,
}: {
  model: ModelInfo;
  selected: boolean;
  onPick: (m: ModelInfo) => void;
  saving: boolean;
}) {
  return (
    <button className="row" onClick={() => onPick(model)} disabled={saving}>
      <span
        style={{
          width: 5,
          height: 5,
          borderRadius: "50%",
          flex: "none",
          background: selected ? "var(--acc)" : "var(--line)",
          transform: "translateY(-3px)",
        }}
      />
      <span style={{ flex: 1, minWidth: 0 }}>
        <span style={{ display: "block", color: selected ? "var(--fg)" : "var(--mut)" }}>
          {model.name}
          {model.id === DEFAULT_MODEL && (
            <span style={{ color: "var(--faint)", fontSize: "var(--fs-caption)", marginLeft: 10 }}>
              Default
            </span>
          )}
        </span>
        <span
          className="mono"
          style={{
            display: "block",
            fontSize: "var(--fs-caption)",
            color: "var(--faint)",
            marginTop: 4,
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
        >
          {model.id}
        </span>
      </span>
      {/* Schema support isn't required, so it's a note rather than a gate. */}
      {model.structured && (
        <span
          style={{ fontSize: "var(--fs-caption)", color: "var(--faint)", flex: "none" }}
          title="Takes a JSON schema"
        >
          schema
        </span>
      )}
      <span
        className="mono"
        style={{
          fontSize: "var(--fs-caption)",
          color: "var(--faint)",
          flex: "none",
          width: 72,
          textAlign: "right",
        }}
      >
        {tokens(model.context)}
      </span>
      <span
        className="mono"
        style={{
          fontSize: "var(--fs-caption)",
          color: "var(--faint)",
          flex: "none",
          width: 106,
          textAlign: "right",
        }}
      >
        {price(model)}
      </span>
    </button>
  );
}

/** Every word has to appear somewhere, so "claude haiku" finds the right one. */
function search(models: ModelInfo[], q: string): ModelInfo[] {
  const terms = q.toLowerCase().split(/\s+/).filter(Boolean);
  if (terms.length === 0) return models;
  return models.filter((m) => {
    const hay = `${m.name} ${m.id}`.toLowerCase();
    return terms.every((t) => hay.includes(t));
  });
}

const tokens = (n: number) => (n >= 1000 ? `${Math.round(n / 1000)}k ctx` : `${n} ctx`);

/** Per million tokens, in and out — the unit OpenRouter's own pricing uses. */
function price(m: ModelInfo): string {
  if (m.promptPerM === 0 && m.completionPerM === 0) return "free";
  const fmt = (n: number) => (n < 1 ? `$${n.toFixed(2)}` : `$${n.toFixed(n < 10 ? 1 : 0)}`);
  return `${fmt(m.promptPerM)}/${fmt(m.completionPerM)}`;
}
