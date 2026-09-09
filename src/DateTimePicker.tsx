import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { CalendarDays, ChevronLeft, ChevronRight, Clock3, X } from "lucide-react";
import type { ClockFormat } from "./types";

interface DateTimePickerProps {
  value: string;
  onChange: (value: string) => void;
  clockFormat: ClockFormat;
}

type ClockPhase = "hour" | "minute";

const weekDays = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const months = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

export function DateTimePicker({ value, onChange, clockFormat }: DateTimePickerProps) {
  const [open, setOpen] = useState(false);
  const [phase, setPhase] = useState<ClockPhase>("hour");
  const [draft, setDraft] = useState(() => parseLocal(value) ?? defaultDueDate());
  const [month, setMonth] = useState(() => startOfMonth(parseLocal(value) ?? new Date()));
  const root = useRef<HTMLDivElement>(null);
  const popover = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (open) return;
    const parsed = parseLocal(value);
    if (parsed) {
      setDraft(parsed);
      setMonth(startOfMonth(parsed));
    }
  }, [open, value]);

  useEffect(() => {
    if (!open) return;
    const closeOutside = (event: PointerEvent) => {
      const target = event.target as Node;
      if (!root.current?.contains(target) && !popover.current?.contains(target)) setOpen(false);
    };
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("pointerdown", closeOutside);
    document.addEventListener("keydown", closeOnEscape);
    return () => {
      document.removeEventListener("pointerdown", closeOutside);
      document.removeEventListener("keydown", closeOnEscape);
    };
  }, [open]);

  const days = useMemo(() => calendarDays(month), [month]);
  const parsedValue = parseLocal(value);
  const displayValue = open ? draft : parsedValue;
  const display = displayValue ? formatDisplay(displayValue, clockFormat) : "Choose date and time";
  const hour12 = draft.getHours() % 12 || 12;
  const period = draft.getHours() >= 12 ? "PM" : "AM";

  function updateDraft(next: Date) {
    setDraft(next);
  }

  function toggle() {
    if (!open && !value) {
      const next = defaultDueDate();
      setDraft(next);
      setMonth(startOfMonth(next));
    }
    if (!open) setPhase("hour");
    setOpen((current) => !current);
  }

  function chooseDay(day: Date) {
    const next = new Date(draft);
    next.setFullYear(day.getFullYear(), day.getMonth(), day.getDate());
    updateDraft(next);
  }

  function chooseHour(hour: number) {
    const next = new Date(draft);
    if (clockFormat === "24h") next.setHours(hour);
    else next.setHours((hour % 12) + (period === "PM" ? 12 : 0));
    updateDraft(next);
    setPhase("minute");
  }

  function choosePeriod(nextPeriod: "AM" | "PM") {
    const next = new Date(draft);
    if (nextPeriod !== period) next.setHours((next.getHours() + 12) % 24);
    updateDraft(next);
  }

  function chooseMinute(minute: number) {
    const next = new Date(draft);
    next.setMinutes(Math.max(0, Math.min(59, minute)), 0, 0);
    updateDraft(next);
  }

  const clockValues = phase === "minute"
    ? Array.from({ length: 12 }, (_, index) => ({ value: index * 5, label: String(index * 5).padStart(2, "0"), ring: "outer" }))
    : clockFormat === "24h"
      ? [
          ...Array.from({ length: 12 }, (_, index) => ({ value: index + 1, label: String(index + 1), ring: "outer" })),
          ...Array.from({ length: 12 }, (_, index) => ({ value: (index + 13) % 24, label: String((index + 13) % 24).padStart(2, "0"), ring: "inner" })),
        ]
      : Array.from({ length: 12 }, (_, index) => ({ value: index + 1, label: String(index + 1), ring: "outer" }));
  const selectedClockValue = phase === "minute" ? Math.floor(draft.getMinutes() / 5) * 5 : clockFormat === "24h" ? draft.getHours() : hour12;
  const handAngle = phase === "minute" ? draft.getMinutes() * 6 : (draft.getHours() % 12) * 30;
  const innerHour = phase === "hour" && clockFormat === "24h" && (draft.getHours() === 0 || draft.getHours() > 12);
  const popoverStyle = open ? pickerPosition(root.current) : undefined;

  return <div className="date-time-picker" ref={root}>
    <button type="button" className={`date-time-trigger ${open ? "active" : ""}`} onClick={toggle}>
      <CalendarDays size={16} /><span>{display}</span><Clock3 size={15} />
    </button>
    {open && createPortal(<div className="date-time-popover" ref={popover} style={popoverStyle} onPointerDown={(event) => event.stopPropagation()}>
      <div className="calendar-panel">
        <div className="picker-heading"><button type="button" onClick={() => setMonth(new Date(month.getFullYear(), month.getMonth() - 1, 1))}><ChevronLeft size={17} /></button><strong>{months[month.getMonth()]} {month.getFullYear()}</strong><button type="button" onClick={() => setMonth(new Date(month.getFullYear(), month.getMonth() + 1, 1))}><ChevronRight size={17} /></button></div>
        <div className="calendar-grid">{weekDays.map((day) => <span className="weekday" key={day}>{day}</span>)}{days.map((day) => {
          const selected = sameDay(day, draft) && Boolean(value);
          const today = sameDay(day, new Date());
          return <button type="button" key={day.toISOString()} className={`${day.getMonth() !== month.getMonth() ? "outside" : ""} ${today ? "today" : ""} ${selected ? "selected" : ""}`} onClick={() => chooseDay(day)}>{day.getDate()}</button>;
        })}</div>
      </div>
      <div className="clock-panel">
        <div className="clock-mode"><button type="button" className={phase === "hour" ? "active" : ""} onClick={() => setPhase("hour")}>{clockFormat === "24h" ? String(draft.getHours()).padStart(2, "0") : hour12}</button><span>:</span><button type="button" className={phase === "minute" ? "active" : ""} onClick={() => setPhase("minute")}>{String(draft.getMinutes()).padStart(2, "0")}</button>{clockFormat === "12h" && <span className="clock-period">{period}</span>}</div>
        <div className={`clock-face ${clockFormat} ${phase}`}>
          <span className="clock-hand" style={{ height: innerHour ? 38 : 61, transform: `translateX(-50%) rotate(${handAngle}deg)` }} />
          {clockValues.map(({ value: clockValue, label, ring }) => {
            const position = phase === "hour" && clockFormat === "24h" && ring === "inner" ? (clockValue === 0 ? 12 : clockValue - 12) : phase === "minute" ? clockValue / 5 || 12 : clockValue;
            const angle = position * Math.PI / 6;
            const radius = ring === "inner" ? 25 : 40;
            const style = { left: `${50 + Math.sin(angle) * radius}%`, top: `${50 - Math.cos(angle) * radius}%` };
            return <button type="button" key={`${ring}-${clockValue}`} style={style} className={`${ring} ${clockValue === selectedClockValue ? "selected" : ""}`} onClick={() => phase === "hour" ? chooseHour(clockValue) : chooseMinute(clockValue)}>{label}</button>;
          })}
          <span className="clock-center" />
        </div>
        <div className="time-controls"><input aria-label="Hour" type="number" min={clockFormat === "24h" ? 0 : 1} max={clockFormat === "24h" ? 23 : 12} value={clockFormat === "24h" ? draft.getHours() : hour12} onChange={(event) => chooseHour(Number(event.target.value))} /><span>:</span><input aria-label="Minute" type="number" min="0" max="59" value={String(draft.getMinutes()).padStart(2, "0")} onChange={(event) => chooseMinute(Number(event.target.value))} />{clockFormat === "12h" && <div className="period-switch"><button type="button" className={period === "AM" ? "active" : ""} onClick={() => choosePeriod("AM")}>AM</button><button type="button" className={period === "PM" ? "active" : ""} onClick={() => choosePeriod("PM")}>PM</button></div>}</div>
      </div>
      <div className="picker-footer"><button type="button" className="clear-date" onClick={() => { onChange(""); setOpen(false); }}><X size={14} />Clear</button><button type="button" className="primary-button" onClick={() => { onChange(formatLocal(draft)); setOpen(false); }}>Done</button></div>
    </div>, document.body)}
  </div>;
}

function pickerPosition(anchor: HTMLDivElement | null): React.CSSProperties {
  if (!anchor) return {};
  const rect = anchor.getBoundingClientRect();
  const width = Math.min(500, window.innerWidth - 24);
  const estimatedHeight = 350;
  const left = Math.max(12, Math.min(rect.left, window.innerWidth - width - 12));
  const top = rect.bottom + 8 + estimatedHeight <= window.innerHeight
    ? rect.bottom + 8
    : Math.max(12, rect.top - estimatedHeight - 8);
  return { position: "fixed", left, top, width };
}

function parseLocal(value: string): Date | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})$/.exec(value);
  if (!match) return null;
  const [, year, month, day, hour, minute] = match;
  const result = new Date(Number(year), Number(month) - 1, Number(day), Number(hour), Number(minute));
  return Number.isNaN(result.getTime()) ? null : result;
}

function formatLocal(date: Date): string {
  const part = (value: number) => String(value).padStart(2, "0");
  return `${date.getFullYear()}-${part(date.getMonth() + 1)}-${part(date.getDate())}T${part(date.getHours())}:${part(date.getMinutes())}`;
}

export function formatDisplay(date: Date, clockFormat: ClockFormat): string {
  return new Intl.DateTimeFormat(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    hour12: clockFormat === "12h",
  }).format(date);
}

function defaultDueDate(): Date {
  const date = new Date(Date.now() + 60 * 60_000);
  date.setSeconds(0, 0);
  date.setMinutes(Math.ceil(date.getMinutes() / 5) * 5);
  return date;
}

function startOfMonth(date: Date): Date {
  return new Date(date.getFullYear(), date.getMonth(), 1);
}

function calendarDays(month: Date): Date[] {
  const firstWeekday = (month.getDay() + 6) % 7;
  const start = new Date(month.getFullYear(), month.getMonth(), 1 - firstWeekday);
  return Array.from({ length: 42 }, (_, index) => new Date(start.getFullYear(), start.getMonth(), start.getDate() + index));
}

function sameDay(left: Date, right: Date): boolean {
  return left.getFullYear() === right.getFullYear() && left.getMonth() === right.getMonth() && left.getDate() === right.getDate();
}
