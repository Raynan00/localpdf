import * as ToggleGroup from "@radix-ui/react-toggle-group";

export interface Chip<T extends string> {
  value: T;
  label: string;
}

/** Single-choice chip row. Radix supplies roving focus and ARIA; the look is ours. */
export function Chips<T extends string>(props: {
  label: string;
  value: T;
  options: Chip<T>[];
  onChange: (v: T) => void;
}) {
  return (
    <div className="field">
      <div className="label">{props.label}</div>
      <ToggleGroup.Root
        type="single"
        className="chips"
        aria-label={props.label}
        value={props.value}
        onValueChange={(v) => v && props.onChange(v as T)}
      >
        {props.options.map((o) => (
          <ToggleGroup.Item key={o.value} value={o.value} className="chip" type="button">
            {o.label}
          </ToggleGroup.Item>
        ))}
      </ToggleGroup.Root>
    </div>
  );
}
