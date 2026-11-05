import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { departmentName, type Department } from "@/lib/api"

/** An agent's department: one of the nine or one the founder added in
    Settings (decision 79). One read in other words shows as itself. */
export function DepartmentField({ id, value, onChange, departments }: {
  id: string; value: string; onChange: (v: string) => void; departments: Department[]
}) {
  const known = departments.some((d) => d.key === value)
  return (
    <Select value={value} onValueChange={(v) => onChange(String(v ?? ""))}>
      <SelectTrigger id={id} className="w-full"><SelectValue placeholder="Choose">{(v: string) => (v ? departmentName(v) : "Choose")}</SelectValue></SelectTrigger>
      <SelectContent>
        {!known && value && <SelectItem value={value}>{departmentName(value)}</SelectItem>}
        {departments.map((d) => <SelectItem key={d.key} value={d.key}>{d.name}</SelectItem>)}
      </SelectContent>
    </Select>
  )
}
