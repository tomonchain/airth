import { Input } from "@/components/ui/input"
import { DEPARTMENTS, departmentName } from "@/lib/api"

/** An agent's department: one of the seven, one the business already has,
    or a new one, written in (decision 79). */
export function DepartmentField({ id, value, onChange, departments = [] }: {
  id: string; value: string; onChange: (v: string) => void; departments?: string[]
}) {
  const known = [...new Set([...DEPARTMENTS, ...departments])]
  return (
    <>
      <Input id={id} list={`${id}-list`} value={value} maxLength={40} autoComplete="off" placeholder="product, or a new one"
        onChange={(e) => onChange(e.target.value)} />
      <datalist id={`${id}-list`}>{known.map((d) => <option key={d} value={d}>{departmentName(d)}</option>)}</datalist>
    </>
  )
}
