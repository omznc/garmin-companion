/**
 * Web Mercator, and the raster tiles that go under a trace.
 *
 * The map used to be a bare shape on the page. A shape tells you the route was
 * a loop; it doesn't tell you it was the loop round the lake, or that the climb
 * at 4 km is the road out of town. So there are streets under it now.
 *
 * Two consequences worth being deliberate about. The projection has to be Web
 * Mercator rather than the flat cos(latitude) squeeze the drawing used, because
 * that is the projection the tiles are cut in and anything else would slide the
 * trace off the roads. And this is the one thing in the app that reaches the
 * network to draw a screen — so it is a toggle, it is attributed, and when it
 * can't load, the drawing underneath is still the whole answer.
 *
 * Tiles are OpenStreetMap data, served in CARTO's near-greyscale styling. A
 * full-colour basemap would fight the trace for attention and lose the one
 * thing the trace is drawn to say.
 */

/** Tile edge, in the tiles' own pixels. */
const TILE = 256;

/** Past this many tiles the view zooms out a step rather than fetching them. */
const MAX_TILES = 48;

/** Deepest zoom worth asking for. Past this the tiles stop having detail. */
const MAX_ZOOM = 19;

export interface Tile {
  key: string;
  href: string;
  /** Top-left corner and edge length, in viewBox units. */
  x: number;
  y: number;
  size: number;
}

/** Longitude/latitude to the unit square Mercator wraps the world onto. */
export function mercator(lat: number, lon: number): { x: number; y: number } {
  const x = (lon + 180) / 360;
  // Clamped short of the poles, where the projection runs off to infinity.
  const s = Math.min(Math.max(Math.sin((lat * Math.PI) / 180), -0.9999), 0.9999);
  const y = 0.5 - Math.log((1 + s) / (1 - s)) / (4 * Math.PI);
  return { x, y };
}

export interface Frame {
  /** viewBox units per unit of world. The world is one unit across. */
  scale: number;
  /** World coordinate at the left/top edge of the drawn route's bounds. */
  minX: number;
  minY: number;
  /** Where those bounds sit in the viewBox. */
  offsetX: number;
  offsetY: number;
  width: number;
  height: number;
}

/**
 * The tiles covering a frame, at the deepest zoom that stays under the budget.
 *
 * Zoom is chosen so one tile lands near its own 256 units in the viewBox — a
 * texture drawn about life size. `@2x` is then asked for on top of that, which
 * is what keeps the labels crisp on a display that has the pixels for them.
 */
export function tilesFor(frame: Frame, style: "light" | "dark"): Tile[] {
  const { scale, minX, minY, offsetX, offsetY, width, height } = frame;
  if (!isFinite(scale) || scale <= 0) return [];

  // A world `scale` units across is 2^z tiles of TILE units each.
  let z = Math.round(Math.log2(scale / TILE));
  z = Math.min(Math.max(z, 0), MAX_ZOOM);

  // The frame in world coordinates, which is wider than the route's own bounds
  // by however much padding and letterboxing the box added around it.
  const worldX = (v: number) => minX + (v - offsetX) / scale;
  const worldY = (v: number) => minY + (v - offsetY) / scale;
  const left = worldX(0);
  const right = worldX(width);
  const top = worldY(0);
  const bottom = worldY(height);

  let x0 = 0;
  let x1 = 0;
  let y0 = 0;
  let y1 = 0;
  for (; z >= 0; z--) {
    const n = 2 ** z;
    x0 = Math.floor(left * n);
    x1 = Math.floor(right * n);
    y0 = Math.max(Math.floor(top * n), 0);
    y1 = Math.min(Math.floor(bottom * n), n - 1);
    if ((x1 - x0 + 1) * (y1 - y0 + 1) <= MAX_TILES) break;
  }
  if (z < 0) return [];

  const n = 2 ** z;
  const size = scale / n;
  const sheet = style === "dark" ? "dark_all" : "light_all";
  const out: Tile[] = [];

  for (let ty = y0; ty <= y1; ty++) {
    for (let tx = x0; tx <= x1; tx++) {
      // The route may straddle the antimeridian, or sit at a longitude the
      // padding pushed past one end of the world. Wrapped, not dropped.
      const wrapped = ((tx % n) + n) % n;
      const sub = "abc"[Math.abs(wrapped + ty) % 3];
      out.push({
        key: `${z}/${tx}/${ty}`,
        href: `https://${sub}.basemaps.cartocdn.com/${sheet}/${z}/${wrapped}/${ty}@2x.png`,
        x: offsetX + (tx / n - minX) * scale,
        y: offsetY + (ty / n - minY) * scale,
        size,
      });
    }
  }
  return out;
}

/** Required by both licences wherever the tiles are shown. */
export const TILE_CREDIT = "© OpenStreetMap contributors · © CARTO";
