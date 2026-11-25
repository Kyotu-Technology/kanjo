export type RangePreset = 'today' | 'last_7' | 'last_30' | 'last_90' | 'month_to_date' | 'last_month' | 'custom';

export const PRESETS: readonly Exclude<RangePreset, 'custom'>[] = ['today', 'last_7', 'last_30', 'last_90', 'month_to_date', 'last_month'];

/** A half-open [from, to) range in epoch ms, in local calendar days. */
export interface DateRange {
  preset: RangePreset;
  from: number;
  to: number;
}

function dayStart(ts: number): number {
  const d = new Date(ts);
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

function addDays(ts: number, n: number): number {
  const d = new Date(ts);
  d.setDate(d.getDate() + n);
  return d.getTime();
}

export function rangeFor(preset: Exclude<RangePreset, 'custom'>, now: number): DateRange {
  const today = dayStart(now);
  const tomorrow = addDays(today, 1);
  const d = new Date(now);
  switch (preset) {
    case 'today':
      return { preset, from: today, to: tomorrow };
    case 'last_7':
      return { preset, from: addDays(today, -6), to: tomorrow };
    case 'last_30':
      return { preset, from: addDays(today, -29), to: tomorrow };
    case 'last_90':
      return { preset, from: addDays(today, -89), to: tomorrow };
    case 'month_to_date':
      return { preset, from: new Date(d.getFullYear(), d.getMonth(), 1).getTime(), to: tomorrow };
    case 'last_month':
      return { preset, from: new Date(d.getFullYear(), d.getMonth() - 1, 1).getTime(), to: new Date(d.getFullYear(), d.getMonth(), 1).getTime() };
  }
}

/** `YYYY-MM-DD` in local time, for date inputs. */
export function toDateInput(ts: number): string {
  const d = new Date(ts);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

/** Custom range from two inclusive date inputs; null when invalid. */
export function customRange(fromInput: string, toInput: string): DateRange | null {
  const re = /^(\d{4})-(\d{2})-(\d{2})$/;
  const a = re.exec(fromInput);
  const b = re.exec(toInput);
  if (!a || !b) return null;
  const from = new Date(Number(a[1]), Number(a[2]) - 1, Number(a[3])).getTime();
  const to = addDays(new Date(Number(b[1]), Number(b[2]) - 1, Number(b[3])).getTime(), 1);
  if (!(to > from)) return null;
  return { preset: 'custom', from, to };
}
