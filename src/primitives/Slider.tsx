export interface SliderProps {
  label: string;
  value: number;
  onChange: (value: number) => void;
  min?: number;
  max?: number;
  step?: number;
  suffix?: string;
  disabled?: boolean;
}

export function Slider({
  label,
  value,
  onChange,
  min = -100,
  max = 100,
  step = 1,
  suffix = '',
  disabled = false,
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
        disabled={disabled}
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
