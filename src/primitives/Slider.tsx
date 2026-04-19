export interface SliderProps {
  label: string;
  value: number;
  onChange: (value: number) => void;
  min?: number;
  max?: number;
  step?: number;
  suffix?: string;
}

export function Slider({
  label,
  value,
  onChange,
  min = -100,
  max = 100,
  step = 1,
  suffix = '',
}: SliderProps) {
  return (
    <div className="slider-row">
      <div className="lbl">{label}</div>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        aria-label={label}
      />
      <div className="val">
        {value > 0 ? `+${value}` : value}
        {suffix}
      </div>
    </div>
  );
}
