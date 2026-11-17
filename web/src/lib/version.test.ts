import { describe, expect, it } from "vitest"
import { parseVersion } from "./version.ts"

describe("parseVersion", () => {
  it("splits the release from the build", () => {
    expect(parseVersion("0.11.0 (8f4a4b4)")).toEqual({ release: "0.11.0", build: "8f4a4b4" })
    expect(parseVersion("0.11.0 (8f4a4b4+)")).toEqual({ release: "0.11.0", build: "8f4a4b4+" })
    expect(parseVersion("0.11.0")).toEqual({ release: "0.11.0", build: null })
    expect(parseVersion("")).toBeNull()
    expect(parseVersion(undefined)).toBeNull()
  })
})
