<script lang="ts">
  let {
    min = 0,
    max = 100,
    step = 1,
    value = $bindable(0),
    label = "",
    format = (v: number) => v.toFixed(1),
    suffix = "",
    size = 60,
    showValue = true,
    disabled = false,
    class: className = "",
    onchange,
    oninput,
    ...rest
  } = $props<{
    min?: number;
    max?: number;
    step?: number;
    value?: number;
    label?: string;
    format?: (v: number) => string;
    suffix?: string;
    size?: number;
    showValue?: boolean;
    disabled?: boolean;
    class?: string;
    onchange?: (value: number) => void;
    oninput?: (value: number) => void;
    [key: string]: any;
  }>();

  let isDragging = $state(false);
  let knobElement: HTMLElement;
  let cx = 0;
  let cy = 0;

  function updateValueFromPointer(e: PointerEvent) {
    if (disabled) return;
    let a = Math.atan2(e.clientY - cy, e.clientX - cx) * 180 / Math.PI;
    
    if (a < 0) a += 360;
    
    // Our track goes from 135 to 405 (which wraps to 45).
    // Let's shift the angle to make it continuous from 135 to 405
    if (a >= 0 && a <= 90) a += 360;
    
    // Dead zone handling (between 90 and 135 degrees)
    if (a > 90 && a < 135) {
      if (a < 112.5) a = 405;
      else a = 135;
    }
    
    let normalized = (a - 135) / 270;
    let newValue = min + normalized * (max - min);
    
    if (step) {
      newValue = Math.round(newValue / step) * step;
    }
    
    newValue = Math.max(min, Math.min(max, newValue));
    
    if (value !== newValue) {
      value = newValue;
      oninput?.(value);
    }
  }

  function handlePointerDown(e: PointerEvent) {
    if (disabled || e.button !== 0) return; // Only left click
    e.preventDefault();
    isDragging = true;
    
    if (knobElement) {
      const rect = knobElement.getBoundingClientRect();
      cx = rect.left + rect.width / 2;
      cy = rect.top + rect.height / 2;
    }
    
    updateValueFromPointer(e);
  }

  function handlePointerMove(e: PointerEvent) {
    if (!isDragging || disabled) return;
    e.preventDefault();
    updateValueFromPointer(e);
  }

  function handlePointerUp(e: PointerEvent) {
    if (isDragging) {
      isDragging = false;
      onchange?.(value);
    }
  }
  
  let normalizedValue = $derived((value - min) / (max - min));
  let dashArray = $derived(2 * Math.PI * 40); // 40 is track radius
  let activeDash = $derived(normalizedValue * dashArray * 0.75);
  
</script>

<svelte:window 
  onpointermove={handlePointerMove} 
  onpointerup={handlePointerUp} 
  onpointercancel={handlePointerUp} 
/>

<div 
  class="relative inline-flex flex-col items-center select-none {disabled ? 'opacity-50 pointer-events-none' : ''} {className}" 
  {...rest}
>
  <div 
    bind:this={knobElement}
    class="relative cursor-grab active:cursor-grabbing group focus:outline-none"
    class:cursor-grabbing={isDragging}
    onpointerdown={handlePointerDown}
    role="slider"
    aria-valuemin={min}
    aria-valuemax={max}
    aria-valuenow={value}
    tabindex="0"
    style="width: {size}px; height: {size}px;"
  >
    <!-- We use SVG viewBox to easily draw a 100x100 circle regardless of actual size -->
    <svg width="100%" height="100%" viewBox="0 0 100 100" class="overflow-visible drop-shadow-sm">
      <!-- Background track -->
      <circle 
        cx="50" cy="50" r="40" 
        fill="none" 
        class="stroke-brand-border/60"
        stroke-width="8" 
        stroke-linecap="round"
        stroke-dasharray="{dashArray * 0.75} {dashArray * 0.25}"
        stroke-dashoffset={0}
        transform="rotate(135 50 50)"
      />
      <!-- Active track -->
      <circle 
        cx="50" cy="50" r="40" 
        fill="none" 
        class="stroke-brand-accent transition-colors group-hover:stroke-brand-accent-hover"
        stroke-width="8" 
        stroke-linecap="round"
        stroke-dasharray="{activeDash} {dashArray}"
        stroke-dashoffset={0}
        transform="rotate(135 50 50)"
      />
      <!-- Inner dial base -->
      <circle cx="50" cy="50" r="30" class="fill-brand-sidebar stroke-brand-border" stroke-width="2" />
      
      <!-- Indicator dot -->
      <g style="transform: rotate({135 + normalizedValue * 270}deg); transform-origin: 50px 50px;">
         <circle cx="75" cy="50" r="4" class="fill-brand-accent transition-colors group-hover:fill-brand-accent-hover" />
      </g>
    </svg>
  </div>
  
  {#if showValue}
  <div class="flex flex-col items-center mt-3 gap-0.5 pointer-events-none">
    {#if label}
      <span class="text-[10px] font-bold text-brand-text-secondary uppercase tracking-wider">{label}</span>
    {/if}
    <span class="text-xs font-mono font-medium {value > 0 ? 'text-green-400' : value < 0 ? 'text-red-400' : 'text-brand-text-primary'}">
      {value > 0 ? "+" : ""}{format(value)}{suffix ? ` ${suffix}` : ""}
    </span>
  </div>
  {/if}
</div>
