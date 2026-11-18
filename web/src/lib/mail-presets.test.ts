import { describe, expect, it } from "vitest"
import { PRESETS, guess, matching, preset } from "./mail-presets.ts"

describe("mail providers' presets", () => {
  it("guesses a provider from an obvious domain only", () => {
    expect(guess("ann@gmail.com")).toBe("gmail")
    expect(guess(" Ann@Hotmail.FR ")).toBe("microsoft")
    expect(guess("ann@me.com")).toBe("icloud")
    expect(guess("ann@fastmail.com")).toBe("fastmail")
    expect(guess("contact@northwind.example")).toBeNull()
    expect(guess("not an address")).toBeNull()
  })

  it("finds the preset a saved mailbox's servers are, or Other", () => {
    for (const p of PRESETS) if (p.servers) expect(matching(p.servers)).toBe(p.id)
    expect(matching({ ...preset("ovh").servers!, smtp_port: 587 })).toBe("other")
    expect(matching({ ...preset("gmail").servers!, imap_host: "IMAP.gmail.com " })).toBe("gmail")
  })

  it("never offers plain text, and OVH comes first, as the default", () => {
    expect(PRESETS[0].id).toBe("ovh")
    expect(preset("nope").id).toBe("ovh")
    for (const p of PRESETS) {
      if (!p.servers) continue
      expect(["ssl", "starttls"]).toContain(p.servers.smtp_security)
      expect(p.servers.imap_port).toBe(993)
    }
  })
})
