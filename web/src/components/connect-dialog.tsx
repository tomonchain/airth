import { useEffect, useRef, useState } from "react"
import { Check, ExternalLink, KeyRound, Loader, ShieldAlert, TriangleAlert } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog"
import { api, enc } from "@/lib/api"
import { LOGOS } from "@/lib/logos"
import { cn } from "@/lib/utils"

/** How a service is connected: its own sign-in page (a remote MCP server,
    decision 20), or a key the founder creates on their side (decision 19). */
export type Method =
  | { kind: "sign-in"; name: string; title: string; url: string; description: string; logo?: string; checked: boolean; tried: boolean }
  | { kind: "key"; id: string; title: string; description: string; help?: string; keyUrl?: string }

type Tool = { name: string; description: string | null }
type Server = { name: string; signed_in: boolean; connecting: boolean; error: string | null; tools?: Tool[] }
type Step = "ask" | "waiting" | "done" | "failed"

const host = (u: string) => { try { return new URL(u).host } catch { return u } }
const hue = (s: string) => [...s].reduce((h, c) => (h * 31 + c.charCodeAt(0)) % 360, 7)

function Tile({ children, className }: { children: React.ReactNode; className?: string }) {
  return <span className={cn("grid size-14 shrink-0 place-items-center rounded-2xl border shadow-sm", className)}>{children}</span>
}

function Mark({ title, logo }: { title: string; logo?: string }) {
  const mark = logo ? LOGOS[logo] : undefined
  if (mark) {
    return (
      <Tile className="bg-white">
        <svg viewBox="0 0 24 24" className="size-7" style={{ color: mark[0] ?? "#111" }} aria-hidden="true"><path fill="currentColor" d={mark[1]} /></svg>
      </Tile>
    )
  }
  const h = hue(title)
  return (
    <Tile className="border-transparent text-xl font-semibold text-white">
      <span className="grid size-full place-items-center rounded-2xl" style={{ background: `linear-gradient(145deg, oklch(0.68 0.14 ${h}), oklch(0.5 0.14 ${h}))` }}>
        {title.replace(/^[^a-z0-9]+/i, "")[0]?.toUpperCase() ?? "?"}
      </span>
    </Tile>
  )
}

/** Airth's ring, as in the favicon. */
function Airth() {
  return (
    <Tile className="bg-white">
      <svg viewBox="0 0 64 64" className="size-8" aria-hidden="true">
        <defs><linearGradient id="airth-ring" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stopColor="#6d5dfc" /><stop offset=".55" stopColor="#3fb8ff" /><stop offset="1" stopColor="#2dd4bf" /></linearGradient></defs>
        <circle cx="32" cy="32" r="22" fill="none" stroke="url(#airth-ring)" strokeWidth="12" />
      </svg>
    </Tile>
  )
}

/** The dots between Airth and the service: still while asking, travelling
    while the link is made, green once it holds, red when it broke. */
function Link({ step }: { step: Step }) {
  return (
    <span className="relative flex w-20 items-center justify-center gap-1.5">
      {step === "done" ? (
        <span className="grid size-6 place-items-center rounded-full bg-emerald-500 text-white duration-300 animate-in zoom-in-50"><Check className="size-3.5" strokeWidth={3} /></span>
      ) : step === "failed" ? (
        <span className="grid size-6 place-items-center rounded-full bg-red-500 text-white duration-300 animate-in zoom-in-50"><TriangleAlert className="size-3.5" strokeWidth={2.5} /></span>
      ) : [0, 1, 2, 3, 4].map((i) => (
        <span key={i} className={cn("size-1 rounded-full bg-sky-500", step === "waiting" ? "animate-[link_1.2s_ease-in-out_infinite]" : "opacity-60")}
          style={{ animationDelay: `${i * 0.12}s` }} />
      ))}
    </span>
  )
}

/** Connecting a service, from the Marketplace: what Airth gets, how it is
    kept, then the link being made and what the agents can now use. */
export function ConnectDialog({ realm, method, onClose, onConnected }: { realm: string; method: Method | null; onClose: () => void; onConnected: () => void }) {
  const [step, setStep] = useState<Step>("ask")
  const [error, setError] = useState<string | null>(null)
  const [signIn, setSignIn] = useState<string | null>(null)
  const [tools, setTools] = useState<Tool[]>([])
  const [note, setNote] = useState<string | null>(null)
  const [key, setKey] = useState("")
  const connected = useRef(onConnected)
  connected.current = onConnected

  useEffect(() => {
    if (!method) return
    setStep("ask"); setError(null); setSignIn(null); setTools([]); setNote(null); setKey("")
  }, [method])

  // While the founder signs in, in their browser: follow the server until it
  // holds a token and has listed its tools, or reports why it could not.
  useEffect(() => {
    if (!method || method.kind !== "sign-in" || step !== "waiting") return
    let live = true
    const look = async () => {
      try {
        const s = (await api<Server[]>(`realms/${enc(realm)}/mcp`)).find((x) => x.name === method.name)
        if (!live || !s) return
        if (s.error) { setError(s.error); setStep("failed") }
        else if (s.signed_in && !s.connecting && (s.tools?.length ?? 0) > 0) { setTools(s.tools ?? []); setStep("done"); connected.current() }
      } catch { /* the next look tries again */ }
    }
    const t = setInterval(look, 1500)
    look()
    return () => { live = false; clearInterval(t) }
  }, [method, step, realm])

  async function start() {
    if (!method) return
    setError(null)
    setStep("waiting")
    try {
      if (method.kind === "sign-in") {
        const out = await api<{ sign_in?: string }>(`realms/${enc(realm)}/connect`, { name: method.name, url: method.url, description: method.description })
        setSignIn(out.sign_in ?? null)
      } else {
        const out = await api<{ check: { ok: boolean; message: string | null } | null }>(`realms/${enc(realm)}/connectors/${enc(method.id)}`, { token: key.trim() })
        onConnected()
        if (out.check && !out.check.ok) { setError(out.check.message); setStep("failed") }
        else { setNote(out.check?.message ?? null); setStep("done") }
      }
    } catch (e) {
      setError((e as Error).message)
      setStep("failed")
    }
  }

  const m = method
  const title = m?.title ?? ""
  return (
    <Dialog open={!!m} onOpenChange={(o) => !o && onClose()}>
      {m && (
        <DialogContent className="gap-0 rounded-3xl p-7 pt-8 text-center sm:max-w-[26rem]">
          <div className="flex items-center justify-center">
            <Airth />
            <Link step={step} />
            <Mark title={title} logo={m.kind === "sign-in" ? m.logo : m.id} />
          </div>

          <DialogTitle className="mt-5 text-lg font-semibold">
            {step === "done" ? `${title} is connected` : step === "failed" ? `${title} is not connected` : `Connect ${title}`}
          </DialogTitle>

          <DialogDescription className="mx-auto mt-2 max-w-sm text-[13.5px] leading-relaxed text-muted-foreground">
            {step === "done"
              ? "Choose which agents may use it in each agent's page: none can until you do."
              : step === "failed" ? (m.kind === "key" ? `${title} refused the key. It is saved: replace it, or try again.` : `The sign-in did not finish.`)
              : m.kind === "sign-in"
                ? step === "waiting"
                  ? `Sign in on the ${title} page that just opened in your browser, then come back here.`
                  : `Sign in to your ${title} account. Airth gets the tools this account allows, without a password. The sign-in stays with Airth and never reaches a model.`
                : `Create a key on ${title} and paste it here. It is kept in ${realm}'s secrets and never reaches a model.`}
          </DialogDescription>

          {/* What only this method, or this service, needs said. */}
          {step === "ask" && m.kind === "sign-in" && !m.checked && (
            <p className="mt-4 flex gap-2 rounded-xl bg-amber-500/10 px-3 py-2.5 text-left text-xs leading-relaxed text-amber-700 dark:text-amber-300">
              <ShieldAlert className="mt-px size-3.5 shrink-0" />
              <span>Not checked by Airth. It answers at <b className="font-mono font-medium">{host(m.url)}</b>: connect it only if {title} itself publishes it.</span>
            </p>
          )}
          {step === "ask" && m.kind === "sign-in" && m.checked && !m.tried && (
            <p className="mt-4 text-xs text-muted-foreground">Official, from {host(m.url)}. Not tried with Airth's sign-in yet.</p>
          )}
          {(step === "ask" || step === "failed") && m.kind === "key" && (
            <div className="mt-5 space-y-3 text-left">
              {m.help && <p className="text-[13px] leading-relaxed text-muted-foreground">{m.help}</p>}
              <div className="flex items-center gap-2 rounded-xl border bg-transparent px-3 focus-within:border-ring focus-within:ring-3 focus-within:ring-ring/30">
                <KeyRound className="size-4 text-muted-foreground" />
                <input type="password" autoComplete="off" spellCheck={false} value={key} onChange={(e) => setKey(e.target.value)} placeholder="Paste the key" autoFocus
                  onKeyDown={(e) => e.key === "Enter" && key.trim() && start()}
                  className="h-10 min-w-0 flex-1 bg-transparent font-mono text-sm outline-none" />
              </div>
              {m.keyUrl && (
                <a href={m.keyUrl} target="_blank" rel="noreferrer" className="inline-flex items-center gap-1 text-xs text-sky-500 hover:underline">Create a key on {title} <ExternalLink className="size-3" /></a>
              )}
            </div>
          )}
          {step === "waiting" && signIn && (
            <a href={signIn} target="_blank" rel="noreferrer" className="mt-3 inline-flex items-center justify-center gap-1 text-xs text-muted-foreground hover:text-foreground">The page did not open? Open it <ExternalLink className="size-3" /></a>
          )}
          {step === "failed" && error && (
            <p className="mt-4 rounded-xl bg-red-500/10 px-3 py-2.5 text-left font-mono text-xs leading-relaxed break-words text-red-700 dark:text-red-300">{error}</p>
          )}
          {step === "done" && (tools.length > 0 || note) && (
            <div className="mt-5 rounded-xl border bg-bubble/40 p-3 text-left duration-300 animate-in fade-in slide-in-from-bottom-1">
              {note && <p className="text-[13px] text-muted-foreground">{note}</p>}
              {tools.length > 0 && (
                <>
                  <p className="mb-2 text-xs text-muted-foreground">{tools.length} tool{tools.length === 1 ? "" : "s"} for the agents</p>
                  <div className="flex flex-wrap gap-1.5">
                    {tools.slice(0, 8).map((t) => <span key={t.name} title={t.description ?? undefined} className="rounded-md bg-muted px-1.5 py-0.5 font-mono text-[11px]">{t.name}</span>)}
                    {tools.length > 8 && <span className="px-1 py-0.5 text-[11px] text-muted-foreground">and {tools.length - 8} more</span>}
                  </div>
                </>
              )}
            </div>
          )}

          <div className="mt-6 flex items-center justify-end gap-2">
            {step === "done" ? (
              <Button className="h-10 flex-1 rounded-xl" onClick={onClose}>Done</Button>
            ) : (
              <>
                <Button variant="ghost" className="h-10 rounded-xl px-5" onClick={onClose}>{step === "waiting" ? "Close" : "Cancel"}</Button>
                <Button className="h-10 flex-1 rounded-xl" disabled={step === "waiting" || (m.kind === "key" && !key.trim())} onClick={start}>
                  {step === "waiting"
                    ? <><Loader className="animate-spin" /> {m.kind === "sign-in" ? "Waiting for the browser…" : "Checking the key…"}</>
                    : step === "failed" ? "Try again" : m.kind === "sign-in" ? `Sign in to ${title}` : "Connect"}
                </Button>
              </>
            )}
          </div>
        </DialogContent>
      )}
    </Dialog>
  )
}
