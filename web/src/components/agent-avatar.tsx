import { BookOpen, Briefcase, Code2, Crown, Eye, Headset, Landmark, Megaphone, PenLine, Settings2, User, type LucideIcon } from "lucide-react"
import type { Agent, AgentState } from "@/lib/api"
import { cn } from "@/lib/utils"

// One hue per department (decision 65), never a status colour.
const DEPT: Record<string, { hue: string; icon: LucideIcon }> = {
  leadership: { hue: "#E07AB8", icon: Crown },
  marketing: { hue: "#F39C4A", icon: Megaphone },
  sales: { hue: "#3FB8C9", icon: Briefcase },
  support: { hue: "#4C8DFF", icon: Headset },
  product: { hue: "#8B6CF6", icon: Code2 },
  operations: { hue: "#8A94A6", icon: Settings2 },
  finance: { hue: "#6C9BD2", icon: Landmark },
}
const ACCESSORY: Record<string, LucideIcon> = { pen: PenLine, lens: Eye, book: BookOpen }

export const STATE_DOT: Record<AgentState, string> = {
  needs: "bg-amber-400",
  incident: "bg-red-500",
  working: "bg-emerald-400",
  idle: "bg-zinc-400 dark:bg-zinc-500",
  asleep: "bg-zinc-300 dark:bg-zinc-700",
}

export function deptHue(department: string | null) {
  return (department && DEPT[department]?.hue) || "#9BA1A9"
}

export function AgentAvatar({ agent, size = "md", className }: { agent: Pick<Agent, "department" | "accessory" | "state">; size?: "sm" | "md" | "lg"; className?: string }) {
  const d = (agent.department && DEPT[agent.department]) || { hue: "#9BA1A9", icon: User }
  const Icon = (agent.accessory && ACCESSORY[agent.accessory]) || d.icon
  const box = { sm: "size-7 rounded-lg", md: "size-10 rounded-xl", lg: "size-14 rounded-2xl" }[size]
  const glyph = { sm: "size-3.5", md: "size-5", lg: "size-7" }[size]
  const dot = { sm: "size-2.5", md: "size-3", lg: "size-3.5" }[size]
  return (
    <span className={cn("relative inline-flex shrink-0", className)}>
      <span
        className={cn("grid place-items-center ring-1 ring-inset ring-white/10", box)}
        style={{ background: `linear-gradient(145deg, ${d.hue}, color-mix(in oklch, ${d.hue}, black 35%))` }}
      >
        <Icon className={cn("text-white drop-shadow-sm", glyph)} strokeWidth={2.2} />
      </span>
      <span className={cn("absolute -right-0.5 -bottom-0.5 rounded-full ring-2 ring-background", dot, STATE_DOT[agent.state])}>
        {agent.state === "working" && <span className="absolute inset-0 animate-ping rounded-full bg-emerald-400/70" />}
      </span>
    </span>
  )
}
