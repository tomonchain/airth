import fs from "node:fs"
import path from "node:path"
import tailwindcss from "@tailwindcss/vite"
import react from "@vitejs/plugin-react"
import { defineConfig } from "vite"

// `npm run dev` talks to a running Airth. Give it the home of a copy
// (AIRTH_HOME); its server.json says the port and the key. Never the real
// home: nothing here defaults to ~/.airth.
function airth() {
  const home = process.env.AIRTH_HOME
  if (!home) return { target: process.env.AIRTH_URL ?? "http://127.0.0.1:7830", token: "" }
  const s = JSON.parse(fs.readFileSync(path.join(home, "server.json"), "utf8"))
  return { target: `http://127.0.0.1:${s.port}`, token: s.token as string }
}

export default defineConfig(() => {
  const a = airth()
  return {
    plugins: [react(), tailwindcss()],
    resolve: { alias: { "@": path.resolve(__dirname, "./src") } },
    server: {
      port: 5173,
      // Airth answers only its own host (decision 37): the proxy speaks as it.
      proxy: {
        "/api": { target: a.target, changeOrigin: true, headers: { Origin: a.target, ...(a.token ? { "X-Airth-Token": a.token } : {}) } },
      },
    },
  }
})
