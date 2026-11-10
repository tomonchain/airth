import { api, onPhone } from "@/lib/api"

// Notifications on the phone (decision 117): the page added to the home
// screen subscribes with the browser's push service, and Airth pushes what
// needs the founder. iOS allows it from 16.4, and only once Airth is opened
// from the home screen.

const supported = () => "serviceWorker" in navigator && "PushManager" in window && "Notification" in window

/** Where this phone stands. */
export type PushState = "unsupported" | "home-screen" | "blocked" | "off" | "on"

const standalone = () =>
  matchMedia("(display-mode: standalone)").matches || (navigator as { standalone?: boolean }).standalone === true

/** Register the worker once, on the phone, and follow the pages its notifications open. */
export function startWorker() {
  if (!onPhone || !("serviceWorker" in navigator)) return
  navigator.serviceWorker.register("/sw.js").catch(() => {})
  navigator.serviceWorker.addEventListener("message", (e) => {
    const open = (e.data as { open?: string } | null)?.open
    if (!open) return
    const u = new URL(open)
    if (u.origin === location.origin) location.hash = u.hash
  })
}

async function current(): Promise<PushSubscription | null> {
  const reg = await navigator.serviceWorker.getRegistration("/")
  return reg ? reg.pushManager.getSubscription() : null
}

export async function pushState(): Promise<PushState> {
  if (!supported()) return standalone() ? "unsupported" : "home-screen"
  if (Notification.permission === "denied") return "blocked"
  return (await current()) ? "on" : "off"
}

function bytes(b64url: string): Uint8Array<ArrayBuffer> {
  const b64 = b64url.replace(/-/g, "+").replace(/_/g, "/") + "=".repeat((4 - (b64url.length % 4)) % 4)
  const raw = atob(b64)
  const out = new Uint8Array(new ArrayBuffer(raw.length))
  for (let i = 0; i < raw.length; i++) out[i] = raw.charCodeAt(i)
  return out
}

const b64url = (buf: ArrayBuffer | null) =>
  buf ? btoa(String.fromCharCode(...new Uint8Array(buf))).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "") : ""

/** Ask the founder, subscribe, and give Airth the address to push to. */
export async function turnOn(): Promise<void> {
  // Asked first, straight from the tap: iOS refuses a request that is not.
  if ((await Notification.requestPermission()) !== "granted") throw new Error("Notifications are not allowed for Airth on this phone.")
  const reg = await navigator.serviceWorker.register("/sw.js")
  await navigator.serviceWorker.ready
  const { key } = await api<{ key: string }>("push")
  let sub = await reg.pushManager.getSubscription()
  if (!sub) sub = await reg.pushManager.subscribe({ userVisibleOnly: true, applicationServerKey: bytes(key) })
  await api("push/subscribe", {
    endpoint: sub.endpoint,
    keys: { p256dh: b64url(sub.getKey("p256dh")), auth: b64url(sub.getKey("auth")) },
  })
}

export async function turnOff(): Promise<void> {
  const sub = await current()
  if (!sub) return
  await api("push/unsubscribe", { endpoint: sub.endpoint }).catch(() => {})
  await sub.unsubscribe()
}
