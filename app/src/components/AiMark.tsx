/**
 * A visible boundary around anything a model wrote. Development builds only.
 *
 * The app mixes two kinds of text that look identical on screen: figures
 * computed in Rust from the cache, and prose a language model produced from
 * those figures. A zone split is arithmetic and is either right or a bug; a
 * sentence about that zone split is a model's reading of it and can be wrong
 * while every number in it is correct. Reading a finished screen, there is
 * nothing to tell you which you are looking at — which is precisely the state
 * in which a stale-data bug hid for two releases, because the wrong answers
 * were well-written and internally consistent.
 *
 * So: in `pnpm tauri dev`, every model-written region gets an accent hairline
 * and a sparkle. Working on the app, you can see at a glance how much of a
 * screen is generated and where the seams are.
 *
 * It disappears entirely in a release build — not hidden with CSS, but gone,
 * returning the children unwrapped so there is no extra element in the tree and
 * no layout to differ between dev and production. That matters more than the
 * bundle size it saves: a debugging affordance that changes the spacing of the
 * thing it wraps is one you stop trusting.
 *
 * `label` says which model output this is. It's worth setting — "chat answer"
 * and "session critique" come from different prompts and fail differently.
 */
import type { ReactNode } from "react";
import { AiIcon } from "../lib/icons";

/** Resolved once at module load; Vite replaces it with a literal and drops the
 *  dev branch from the release bundle. */
const DEV = import.meta.env.DEV;

export function AiMark({
  label = "AI generated",
  children,
}: {
  label?: string;
  children: ReactNode;
}) {
  if (!DEV) return <>{children}</>;

  return (
    <div className="ai-mark" data-label={label}>
      <span className="ai-mark-badge" aria-hidden="true">
        <AiIcon size={11} weight="fill" />
        {label}
      </span>
      {children}
    </div>
  );
}
