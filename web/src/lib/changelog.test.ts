import { test } from "vitest"
import assert from "node:assert/strict"
import { readFileSync } from "node:fs"
import { CHANGELOG } from "./changelog.ts"

const parts = (v: string) => v.split(".").map(Number)
const newer = (a: string, b: string) => {
  const [x, y] = [parts(a), parts(b)]
  for (let i = 0; i < 3; i++) if (x[i] !== y[i]) return x[i] > y[i]
  return false
}

test("the newest entry is the version being built", () => {
  // The version bump's pull request writes the entry: a bump without one fails here.
  const cargo = readFileSync(new URL("../../../Cargo.toml", import.meta.url), "utf8")
  const version = cargo.match(/^version = "([^"]+)"/m)?.[1]
  assert.equal(CHANGELOG[0].version, version)
})

test("entries go from newest to oldest, each once, each saying something", () => {
  for (let i = 1; i < CHANGELOG.length; i++) {
    assert.ok(newer(CHANGELOG[i - 1].version, CHANGELOG[i].version), `${CHANGELOG[i - 1].version} before ${CHANGELOG[i].version}`)
    assert.ok(CHANGELOG[i - 1].date >= CHANGELOG[i].date, `${CHANGELOG[i - 1].version} is dated before ${CHANGELOG[i].version}`)
  }
  for (const e of CHANGELOG) {
    assert.match(e.version, /^\d+\.\d+\.\d+$/)
    assert.match(e.date, /^\d{4}-\d{2}-\d{2}$/)
    assert.ok(e.title.trim())
    assert.ok((e.added?.length ?? 0) + (e.improved?.length ?? 0) + (e.fixed?.length ?? 0) > 0, `${e.version} says nothing`)
  }
})
