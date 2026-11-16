import { useEffect, useRef, useState } from "react"
import { Minus, Plus } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { MAX_ZOOM, centred, moved, scale, source, zoomed, type Crop } from "@/lib/crop"
import { cn } from "@/lib/utils"

// The view the picture is framed in, in CSS pixels, and the saved size.
const VIEW = 280
const OUT = 512
// The server keeps pictures up to 1 MB (src/images.rs).
const LIMIT = 1 << 20

/** The saved square: a PNG, or a JPEG when the PNG would be too large. */
async function encode(canvas: HTMLCanvasElement): Promise<Blob> {
  const as = (type: string, q?: number) => new Promise<Blob | null>((ok) => canvas.toBlob(ok, type, q))
  const png = await as("image/png")
  if (png && png.size <= LIMIT) return png
  for (const q of [0.92, 0.85, 0.75]) {
    const jpg = await as("image/jpeg", q)
    if (jpg && jpg.size <= LIMIT) return jpg
  }
  throw new Error("This picture is too large, even cropped.")
}

/** Frame a picked picture before it is saved: drag to move, zoom with the
    slider (or the wheel), a round or square preview as it will show. */
export function CropDialog({ file, round, onCancel, onSave }: { file: File | null; round?: boolean; onCancel: () => void; onSave: (b: Blob) => Promise<void> }) {
  const [img, setImg] = useState<{ bitmap: ImageBitmap; url: string } | null>(null)
  const [crop, setCrop] = useState<Crop>({ x: 0, y: 0, zoom: 1 })
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const drag = useRef<{ x: number; y: number } | null>(null)

  useEffect(() => {
    if (!file) { setImg(null); return }
    let live = true
    const url = URL.createObjectURL(file)
    setError(null)
    createImageBitmap(file).then(
      (bitmap) => { if (!live) return; setImg({ bitmap, url }); setCrop(centred(bitmap.width, bitmap.height, VIEW)) },
      () => { if (live) setError("This file is not an image Airth can read.") },
    )
    return () => { live = false; URL.revokeObjectURL(url) }
  }, [file])

  const w = img?.bitmap.width ?? 1
  const h = img?.bitmap.height ?? 1
  const s = scale(w, h, VIEW, crop.zoom)

  async function save() {
    if (!img) return
    setBusy(true)
    try {
      const { sx, sy, side } = source(crop, w, h, VIEW)
      const canvas = document.createElement("canvas")
      canvas.width = canvas.height = OUT
      const ctx = canvas.getContext("2d")!
      ctx.imageSmoothingQuality = "high"
      ctx.drawImage(img.bitmap, sx, sy, side, side, 0, 0, OUT, OUT)
      await onSave(await encode(canvas))
    } catch (e) { setError((e as Error).message) } finally { setBusy(false) }
  }

  // The same framing, smaller: how the picture will show.
  const Preview = ({ size }: { size: number }) => {
    const k = size / VIEW
    return (
      <span className={cn("relative block shrink-0 overflow-hidden bg-muted", round ? "rounded-full" : "rounded-xl")} style={{ width: size, height: size }}>
        {img && <img src={img.url} alt="" draggable={false} className="absolute top-0 left-0 max-w-none origin-top-left"
          style={{ width: w, height: h, transform: `translate(${crop.x * k}px, ${crop.y * k}px) scale(${s * k})` }} />}
      </span>
    )
  }

  return (
    <Dialog open={!!file} onOpenChange={(o) => { if (!o && !busy) onCancel() }}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Frame your picture</DialogTitle>
          <DialogDescription>Drag to move it, zoom to fit. What is inside the {round ? "circle" : "square"} is kept.</DialogDescription>
        </DialogHeader>
        <div className="flex flex-col items-center gap-4">
          <div className="relative cursor-grab touch-none overflow-hidden rounded-xl bg-muted select-none active:cursor-grabbing" style={{ width: VIEW, height: VIEW }}
            onPointerDown={(e) => { e.currentTarget.setPointerCapture(e.pointerId); drag.current = { x: e.clientX, y: e.clientY } }}
            onPointerMove={(e) => {
              if (!drag.current || !img) return
              const dx = e.clientX - drag.current.x, dy = e.clientY - drag.current.y
              drag.current = { x: e.clientX, y: e.clientY }
              setCrop((c) => moved(c, dx, dy, w, h, VIEW))
            }}
            onPointerUp={() => { drag.current = null }}
            onWheel={(e) => {
              if (!img) return
              const r = e.currentTarget.getBoundingClientRect()
              setCrop((c) => zoomed(c, c.zoom * (e.deltaY < 0 ? 1.08 : 1 / 1.08), w, h, VIEW, e.clientX - r.left, e.clientY - r.top))
            }}>
            {img && <img src={img.url} alt="Your picture" draggable={false} className="pointer-events-none absolute top-0 left-0 max-w-none origin-top-left"
              style={{ width: w, height: h, transform: `translate(${crop.x}px, ${crop.y}px) scale(${s})` }} />}
            {/* Outside the kept shape, dimmed. */}
            <span aria-hidden className={cn("pointer-events-none absolute inset-0", round ? "rounded-full shadow-[0_0_0_999px_rgb(0_0_0/0.55)]" : "rounded-xl ring-2 ring-white/70 ring-inset")} />
          </div>
          <div className="flex w-full items-center gap-2.5" style={{ maxWidth: VIEW }}>
            <Button size="icon-sm" variant="ghost" aria-label="Zoom out" disabled={!img || crop.zoom <= 1} onClick={() => setCrop((c) => zoomed(c, c.zoom - 0.25, w, h, VIEW))}><Minus /></Button>
            <input type="range" aria-label="Zoom" min={1} max={MAX_ZOOM} step={0.01} value={crop.zoom} disabled={!img}
              onChange={(e) => setCrop((c) => zoomed(c, Number(e.target.value), w, h, VIEW))}
              className="h-1.5 min-w-0 flex-1 cursor-pointer accent-foreground" />
            <Button size="icon-sm" variant="ghost" aria-label="Zoom in" disabled={!img || crop.zoom >= MAX_ZOOM} onClick={() => setCrop((c) => zoomed(c, c.zoom + 0.25, w, h, VIEW))}><Plus /></Button>
          </div>
          <div className="flex items-center gap-3 self-stretch rounded-xl bg-muted/60 px-3 py-2.5">
            <Preview size={44} />
            <span className="text-xs text-muted-foreground">How it will show in Airth.</span>
          </div>
          {error && <p className="self-stretch rounded-lg bg-red-500/10 px-3 py-2 text-xs text-red-600 dark:text-red-300">{error}</p>}
        </div>
        <DialogFooter>
          <Button variant="ghost" disabled={busy} onClick={onCancel}>Cancel</Button>
          <Button disabled={!img || busy} onClick={save}>{busy ? "Saving…" : "Save"}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
