// The Home's greeting and its light: what the founder reads on opening
// Airth changes with the time of day and the day of the week, as a person
// would say it. Kept here, apart from the page, so it is tested.

export type Moment = "morning" | "afternoon" | "evening" | "night"

/** The part of the day, in the founder's local time. */
export function moment(now: Date): Moment {
  const h = now.getHours()
  if (h >= 5 && h < 12) return "morning"
  if (h >= 12 && h < 18) return "afternoon"
  if (h >= 18 && h < 23) return "evening"
  return "night"
}

const WEEKDAYS = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"]

/** The lines a moment may open with; `{name}` is the founder's first name,
    and a line without it reads well alone. */
function lines(now: Date): string[] {
  const m = moment(now)
  const day = now.getDay()
  const weekend = day === 0 || day === 6
  if (m === "night") return ["Working late{, name}?", "Still up{, name}?", "A quiet hour{, name}. What's on your mind?"]
  if (weekend) return [`A calm ${WEEKDAYS[day]}{, name}?`, "Weekend shift{, name}?", "What shall we do today{, name}?"]
  if (m === "morning") {
    if (day === 1) return ["New week{, name}. Where do we start?", "Good morning{, name}. What's first this week?"]
    return ["Good morning{, name}", "What shall we do today{, name}?", "Morning{, name}. What's first?"]
  }
  if (m === "afternoon") {
    if (day === 5) return ["Happy Friday{, name}", "Friday afternoon{, name}. What's left?"]
    return ["Good afternoon{, name}", "What shall we do today{, name}?", "Back at it{, name}?"]
  }
  if (day === 5) return ["Happy Friday{, name}", "Good evening{, name}"]
  return ["Good evening{, name}", "Evening{, name}. Anything before tomorrow?"]
}

/** The greeting for `now`: one of its moment's lines, the same all through
    that moment of that day (it does not change on every visit), with the
    founder's first name when known. */
export function greeting(now: Date, first: string | null): string {
  const options = lines(now)
  const seed = now.getFullYear() * 1000 + dayOfYear(now) * 4 + ["morning", "afternoon", "evening", "night"].indexOf(moment(now))
  const line = options[hash(seed) % options.length]
  return line.replace("{, name}", first ? `, ${first}` : "")
}

function dayOfYear(d: Date): number {
  return Math.floor((Date.UTC(d.getFullYear(), d.getMonth(), d.getDate()) - Date.UTC(d.getFullYear(), 0, 0)) / 86400000)
}

/** A small, stable spread of a number (so consecutive days differ). */
function hash(n: number): number {
  let x = n | 0
  x = Math.imul(x ^ (x >>> 16), 0x45d9f3b)
  x = Math.imul(x ^ (x >>> 16), 0x45d9f3b)
  return (x ^ (x >>> 16)) >>> 0
}

/** The two soft lights behind the greeting, as "r g b": the morning warm,
    the afternoon clear, the evening violet, the night deep blue. */
export function light(now: Date): [string, string] {
  switch (moment(now)) {
    case "morning": return ["251 191 36", "244 114 182"]
    case "afternoon": return ["56 189 248", "45 212 191"]
    case "evening": return ["139 92 246", "244 114 182"]
    case "night": return ["79 70 229", "30 64 175"]
  }
}
