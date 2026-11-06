import { useRef, useState } from "react"
import { Mic } from "lucide-react"
import { toast } from "sonner"
import { cn } from "@/lib/utils"

type Rec = { lang: string; interimResults: boolean; continuous: boolean; start: () => void; stop: () => void
  onresult: (e: { results: ArrayLike<ArrayLike<{ transcript: string }>> }) => void; onerror: (e: { error: string }) => void; onend: () => void }
const SpeechRec = (window as unknown as { SpeechRecognition?: new () => Rec; webkitSpeechRecognition?: new () => Rec }).SpeechRecognition
  ?? (window as unknown as { webkitSpeechRecognition?: new () => Rec }).webkitSpeechRecognition

/** Dictate into a text box (the classic interface's): the speech becomes
    text to edit before sending. Without the browser's recognition, the
    system's dictation does it. */
export function MicButton({ value, onChange, className }: { value: string; onChange: (v: string) => void; className?: string }) {
  const rec = useRef<Rec | null>(null)
  const [on, setOn] = useState(false)
  function toggle() {
    if (!SpeechRec) { toast(/Mac/.test(navigator.platform) ? "Press the 🎤 key, or Fn twice, to dictate with macOS." : "Use your system's dictation to speak."); return }
    if (rec.current) { rec.current.stop(); return }
    const r = new SpeechRec()
    r.lang = navigator.language || "fr-FR"
    r.interimResults = true
    r.continuous = true
    const base = value.trim() ? value.replace(/\s*$/, " ") : ""
    r.onresult = (e) => { let t = ""; for (const x of Array.from(e.results)) t += x[0].transcript; onChange(base + t) }
    r.onerror = (e) => { if (e.error !== "aborted" && e.error !== "no-speech") toast.error(`Dictation: ${e.error}`) }
    r.onend = () => { rec.current = null; setOn(false) }
    try { r.start(); rec.current = r; setOn(true) } catch (e) { toast.error(`Dictation unavailable: ${(e as Error).message}`) }
  }
  return (
    <button type="button" onClick={toggle} title="Dictate" aria-label="Dictate" aria-pressed={on}
      className={cn("grid size-7 place-items-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground", on && "animate-pulse bg-red-500/15 text-red-500", className)}>
      <Mic className="size-4" />
    </button>
  )
}
