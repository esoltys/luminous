<script lang="ts" module>
  export type InputSize = "md" | "sm";
  export type InputSurface = "main" | "sidebar";
</script>

<script lang="ts">
  interface Props {
    value?: string | number | null;
    type?: string;
    id?: string;
    name?: string;
    placeholder?: string;
    disabled?: boolean;
    required?: boolean;
    size?: InputSize;
    surface?: InputSurface;
    /** Swaps the border/ring to the accent color, e.g. to flag a field that was just auto-filled. */
    highlighted?: boolean;
    class?: string;
    style?: string;
    oninput?: (e: Event & { currentTarget: HTMLInputElement }) => void;
    onchange?: (e: Event & { currentTarget: HTMLInputElement }) => void;
    onkeydown?: (e: KeyboardEvent & { currentTarget: HTMLInputElement }) => void;
    onfocus?: (e: FocusEvent & { currentTarget: HTMLInputElement }) => void;
  }

  let {
    value = $bindable(""),
    type = "text",
    id,
    name,
    placeholder,
    disabled = false,
    required = false,
    size = "md",
    surface = "main",
    highlighted = false,
    class: className = "",
    style,
    oninput,
    onchange,
    onkeydown,
    onfocus,
  }: Props = $props();

  const sizeClasses: Record<InputSize, string> = {
    md: "rounded-xl px-3.5 py-2 text-sm font-medium",
    sm: "rounded-lg px-2.5 py-1.5 text-xs",
  };

  const surfaceClasses: Record<InputSurface, string> = {
    main: "bg-brand-main",
    sidebar: "bg-brand-sidebar",
  };
</script>

<input
  {id}
  {name}
  {type}
  bind:value
  {placeholder}
  {disabled}
  {required}
  {oninput}
  {onchange}
  {onkeydown}
  {onfocus}
  {style}
  class="border text-brand-text-primary outline-none focus:border-brand-accent disabled:opacity-50 transition-colors {surfaceClasses[surface]} {sizeClasses[size]} {highlighted ? 'border-brand-accent ring-1 ring-brand-accent/40' : 'border-brand-border'} {className}"
/>
