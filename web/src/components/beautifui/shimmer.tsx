// From Beautiful UI (beautifului.dev, by Turbo), MIT licence; its registry
// item "shimmer", with Airth's text colours.
/** Shimmering label — signals the agent is processing. */
export function Shimmer({
  children,
  className = "",
}: {
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <span
      className={`inline-block bg-clip-text text-transparent ${className}`}
      style={{
        backgroundImage:
          "linear-gradient(90deg, var(--muted-foreground) 35%, var(--foreground) 50%, var(--muted-foreground) 65%)",
        backgroundSize: "200% 100%",
        animation: "shimmer-text 1.8s linear infinite",
      }}
    >
      {children}
    </span>
  );
}
