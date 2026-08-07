<script lang="ts" module>
  export type ButtonVariant = "primary" | "secondary" | "accent-soft" | "info" | "warning" | "destructive";
  export type ButtonSize = "md" | "sm";
</script>

<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    onclick?: (e: MouseEvent) => void;
    type?: "button" | "submit";
    disabled?: boolean;
    title?: string;
    variant?: ButtonVariant;
    size?: ButtonSize;
    class?: string;
    children: Snippet;
  }

  let {
    onclick,
    type = "button",
    disabled = false,
    title,
    variant = "secondary",
    size = "md",
    class: className = "",
    children,
  }: Props = $props();

  const variantClasses: Record<ButtonVariant, string> = {
    primary: "bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast shadow-md shadow-brand-accent/20",
    secondary: "bg-brand-sidebar hover:bg-brand-main border border-brand-border/60 text-brand-text-primary",
    "accent-soft":
      "border border-brand-accent/60 bg-brand-accent/10 hover:bg-brand-accent text-brand-accent-text hover:text-brand-accent-contrast shadow-sm",
    info: "bg-purple-500/10 hover:bg-purple-500/20 border border-purple-500/30 text-purple-400 hover:text-purple-300",
    warning: "bg-amber-500/10 hover:bg-amber-500/20 border border-amber-500/30 text-amber-400 hover:text-amber-300",
    destructive: "bg-red-500/10 hover:bg-red-500/20 border border-red-500/30 text-red-400 hover:text-red-300",
  };

  const sizeClasses: Record<ButtonSize, string> = {
    md: "gap-2 px-5 py-2 text-sm",
    sm: "gap-1.5 px-3 py-1.5 text-xs",
  };
</script>

<button
  {onclick}
  {type}
  {disabled}
  {title}
  class="flex items-center justify-center rounded-full font-semibold transition-colors disabled:opacity-50 disabled:cursor-not-allowed {sizeClasses[size]} {variantClasses[variant]} {className}"
>
  {@render children()}
</button>
