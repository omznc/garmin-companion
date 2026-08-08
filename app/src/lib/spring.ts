/**
 * A spring, in the two parameters Apple's fluid-interface work uses in place of
 * mass/stiffness/damping.
 *
 * A duration-based transition can't be redirected: once it starts it plays out,
 * and new input either waits for it or cuts it off with a visible jump. A spring
 * has no duration — it has a target — so new input is just a new target, and the
 * motion stays continuous through it. That is the whole reason anything the user
 * can touch animates through this rather than through CSS.
 *
 *   damping   1.0 settles without overshoot; below that it bounces. Default,
 *             because overshoot on something that wasn't thrown reads as noise.
 *   response  roughly how long it takes to arrive, in seconds. Not a duration —
 *             the settle time falls out of the two numbers together.
 *
 * Values are plain numbers; the caller decides what they mean and writes them to
 * the DOM. One rAF loop drives every live spring.
 */

export type SpringConfig = { damping?: number; response?: number };

const live = new Set<Spring>();
let frame = 0;
let last = 0;

function tick(now: number) {
  frame = 0;
  // Clamped, so a backgrounded tab or a dropped frame resumes rather than
  // integrating one enormous step and flinging everything off-screen.
  const dt = Math.min((now - last) / 1000, 1 / 30);
  last = now;
  for (const s of live) s.step(dt);
  if (live.size) frame = requestAnimationFrame(tick);
}

function start(s: Spring) {
  if (!live.size) last = performance.now();
  live.add(s);
  if (!frame) frame = requestAnimationFrame(tick);
}

/** Checked per call rather than cached: the setting can change mid-session. */
function reduced(): boolean {
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

export class Spring {
  x = 0;
  v = 0;
  target = 0;
  damping = 1;
  response = 0.35;

  constructor(
    private readonly onChange: (x: number) => void,
    config?: SpringConfig,
  ) {
    this.configure(config);
  }

  private configure(config?: SpringConfig) {
    if (!config) return;
    if (config.damping !== undefined) this.damping = config.damping;
    if (config.response !== undefined) this.response = config.response;
  }

  /**
   * Jump there. No animation and no velocity — this is for seeding a FLIP with
   * the distance a thing just moved, and for tracking a finger, where the
   * pointer is the animation and a spring in the middle would only add lag.
   */
  set(x: number) {
    live.delete(this);
    this.x = this.target = x;
    this.v = 0;
    this.onChange(x);
  }

  /**
   * Aim somewhere new. Velocity carries through, so a reversal blends into the
   * new direction instead of hitting a brick wall; pass one to hand off the
   * speed a gesture ended at, which is what makes the seam between dragging and
   * animating disappear.
   */
  to(target: number, velocity?: number, config?: SpringConfig) {
    this.configure(config);
    this.target = target;
    if (velocity !== undefined) this.v = velocity;
    // Reduced motion still gets the state change, just not the travel.
    if (reduced()) return this.set(target);
    if (this.x === target && this.v === 0) return;
    start(this);
  }

  stop() {
    live.delete(this);
    this.v = 0;
  }

  /** @internal — driven by the shared ticker. */
  step(dt: number) {
    const k = (2 * Math.PI) / this.response;
    const stiffness = k * k;
    const damper = 2 * this.damping * k;
    // Fixed substeps: the integrator is only stable when the step is small
    // relative to the period, and a stiff spring on a slow frame isn't.
    const steps = Math.max(1, Math.ceil(dt * 240));
    const h = dt / steps;
    for (let i = 0; i < steps; i++) {
      const a = -stiffness * (this.x - this.target) - damper * this.v;
      this.v += a * h;
      this.x += this.v * h;
    }
    // Sub-pixel and slowing: nobody can see the rest of the curve.
    if (Math.abs(this.x - this.target) < 0.05 && Math.abs(this.v) < 0.05) {
      this.x = this.target;
      this.v = 0;
      live.delete(this);
    }
    this.onChange(this.x);
  }
}

/**
 * Resistance past a boundary, rather than a wall. A hard stop reads as frozen;
 * something that still moves, just less and less, reads as responsive with
 * nothing more to give.
 */
export function rubberband(over: number, dimension: number, constant = 0.55): number {
  return (over * dimension * constant) / (dimension + constant * Math.abs(over));
}

/**
 * How much further a flick would have carried, given the speed it was released
 * at. Add it to the release position to get where the gesture was *going*, and
 * decide against that rather than against where the finger happened to stop.
 *
 * That difference is what makes a flick feel thrown: a short, fast swipe and a
 * long, slow drag end in the same place, and only one of them meant to let go.
 * The exponential-decay form is the one scroll deceleration actually uses — the
 * textbook `v²/2a` is a different curve and lands short.
 *
 * @param velocity  px per second at release.
 * @param decel     0.998 for scroll's own feel, lower for something snappier.
 */
export function project(velocity: number, decel = 0.998): number {
  return ((velocity / 1000) * decel) / (1 - decel);
}
