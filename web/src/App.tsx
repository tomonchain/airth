import { useEffect, useState } from "react"
import { AppSidebar } from "@/components/app-sidebar"
import { usePoll } from "@/hooks/use-poll"
import { api, type Realm } from "@/lib/api"
import { Floor } from "@/screens/floor"

function route(): string | null {
  const m = location.hash.match(/^#\/b\/([^/?&]+)/)
  return m ? decodeURIComponent(m[1]) : null
}

export default function App() {
  const [realm, setRealm] = useState<string | null>(route)
  useEffect(() => {
    const on = () => setRealm(route())
    addEventListener("hashchange", on)
    return () => removeEventListener("hashchange", on)
  }, [])
  const { data: realms, error } = usePoll(() => api<Realm[]>("realms"), 15000, [])
  const { data: hq } = usePoll(() => api<{ businesses: { name: string; needs: number }[] }>("hq"), 15000, [])
  const current = realm ?? realms?.find((r) => r.default)?.name ?? realms?.[0]?.name ?? null
  const needs = Object.fromEntries((hq?.businesses ?? []).map((b) => [b.name, b.needs]))

  return (
    <div className="flex h-svh overflow-hidden">
      <AppSidebar realms={realms ?? []} current={current} needs={needs} />
      {current ? (
        <Floor realm={current} />
      ) : (
        <main className="grid flex-1 place-items-center text-sm text-muted-foreground">
          {error ? `Airth is not answering: ${error}` : "Loading…"}
        </main>
      )}
    </div>
  )
}
