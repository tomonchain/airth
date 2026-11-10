// Airth's service worker (decision 117). It only shows what Airth pushes and
// opens the page a notification names. It caches nothing: Airth is always
// read live from the server.

self.addEventListener("install", () => self.skipWaiting())
self.addEventListener("activate", (e) => e.waitUntil(self.clients.claim()))

self.addEventListener("push", (e) => {
  let m = {}
  try { m = e.data ? e.data.json() : {} } catch { m = { body: e.data ? e.data.text() : "" } }
  e.waitUntil(self.registration.showNotification(m.title || "Airth", {
    body: m.body || "",
    tag: m.tag,
    icon: "/apple-touch-icon.png",
    data: { url: m.url || "/" },
  }))
})

self.addEventListener("notificationclick", (e) => {
  e.notification.close()
  const url = new URL((e.notification.data && e.notification.data.url) || "/", self.location.origin).href
  e.waitUntil((async () => {
    const open = await self.clients.matchAll({ type: "window", includeUncontrolled: true })
    for (const c of open) {
      if (new URL(c.url).origin !== self.location.origin) continue
      await c.focus()
      // The app follows the address itself, without reloading.
      c.postMessage({ open: url })
      return
    }
    return self.clients.openWindow(url)
  })())
})
