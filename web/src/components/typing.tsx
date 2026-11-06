// The agent at work, as a chat shows someone typing: three dots in a wave.
export function Typing() {
  return (
    <span aria-hidden className="inline-flex items-center gap-[3px]">
      {[0, 1, 2].map((i) => (
        <span key={i} className="size-[6px] rounded-full bg-foreground/60" style={{ animation: `typing 1.2s ease-in-out ${i * 0.15}s infinite` }} />
      ))}
    </span>
  )
}
