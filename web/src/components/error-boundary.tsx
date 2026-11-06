import { Component, type ReactNode } from "react"
import { TriangleAlert } from "lucide-react"
import { Button } from "@/components/ui/button"

/** One page that throws shows why, with a way back, instead of taking the
    whole window down: the rail, the bar and the other pages keep working. */
export class PageBoundary extends Component<{ children: ReactNode; page: string }, { error: Error | null }> {
  state = { error: null as Error | null }

  static getDerivedStateFromError(error: Error) {
    return { error }
  }

  componentDidUpdate(prev: { page: string }) {
    // Another page: a fresh start.
    if (prev.page !== this.props.page && this.state.error) this.setState({ error: null })
  }

  componentDidCatch(error: Error) {
    console.error(`[airth] ${this.props.page}:`, error)
  }

  render() {
    const e = this.state.error
    if (!e) return this.props.children
    return (
      <div className="grid flex-1 place-items-center p-8 text-center">
        <div className="max-w-md">
          <TriangleAlert className="mx-auto size-8 text-amber-500" />
          <p className="mt-3 text-sm font-medium">This page hit a problem.</p>
          <p className="mt-1 font-mono text-xs break-words text-muted-foreground">{e.message}</p>
          <div className="mt-4 flex justify-center gap-2">
            <Button size="sm" onClick={() => this.setState({ error: null })}>Try again</Button>
            <Button size="sm" variant="ghost" onClick={() => location.reload()}>Reload Airth</Button>
          </div>
        </div>
      </div>
    )
  }
}
