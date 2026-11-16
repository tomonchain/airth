// Cropping a picture to a square before it is saved (Settings, Picture):
// the image is shown in a square view of `view` pixels, moved and zoomed
// there, and never leaves a gap in it. The maths are here, so they are
// tested; the dialog is components/crop-dialog.tsx.

/** Where the image sits in the view: its top-left corner (x, y), in view
    pixels, and the zoom over "just covers the view" (1 to MAX_ZOOM). */
export type Crop = { x: number; y: number; zoom: number }

export const MAX_ZOOM = 4

/** View pixels per image pixel at a zoom: at 1 the shorter side fills the view. */
export const scale = (w: number, h: number, view: number, zoom: number) => (view / Math.min(w, h)) * zoom

const within = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v))

/** The crop held inside its bounds: the zoom between 1 and MAX_ZOOM, and the
    image always covering the whole view. */
export function clamp(c: Crop, w: number, h: number, view: number): Crop {
  const zoom = within(c.zoom, 1, MAX_ZOOM)
  const s = scale(w, h, view, zoom)
  return { zoom, x: within(c.x, view - w * s, 0), y: within(c.y, view - h * s, 0) }
}

/** The image centred, just covering the view. */
export function centred(w: number, h: number, view: number): Crop {
  const s = scale(w, h, view, 1)
  return { zoom: 1, x: (view - w * s) / 2, y: (view - h * s) / 2 }
}

/** The crop moved by (dx, dy) view pixels, held inside its bounds. */
export const moved = (c: Crop, dx: number, dy: number, w: number, h: number, view: number) =>
  clamp({ ...c, x: c.x + dx, y: c.y + dy }, w, h, view)

/** The crop at another zoom, keeping the image point under (px, py) (the
    view's centre by default) where it was. */
export function zoomed(c: Crop, zoom: number, w: number, h: number, view: number, px = view / 2, py = view / 2): Crop {
  const z = within(zoom, 1, MAX_ZOOM)
  const before = scale(w, h, view, c.zoom)
  const after = scale(w, h, view, z)
  // The image point under (px, py), in image pixels, stays under it.
  const ix = (px - c.x) / before
  const iy = (py - c.y) / before
  return clamp({ zoom: z, x: px - ix * after, y: py - iy * after }, w, h, view)
}

/** The square of the image the view shows, in image pixels: what is saved. */
export function source(c: Crop, w: number, h: number, view: number): { sx: number; sy: number; side: number } {
  const s = scale(w, h, view, c.zoom)
  return { sx: -c.x / s, sy: -c.y / s, side: view / s }
}
