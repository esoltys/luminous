<script lang="ts" generics="T = any">
  import { onMount, tick, type Snippet } from "svelte";
  import { CaretDownIcon as CaretDown } from "phosphor-svelte";

  export interface ListboxItem<T = any> {
    id: string;
    value: string;
    label: string;
    group?: string;
    disabled?: boolean;
    data?: T;
  }

  export interface ListboxGroup {
    id: string;
    label: string;
  }

  interface Props<T> {
    id?: string;
    value: string | null;
    items: ListboxItem<T>[];
    groups?: ListboxGroup[];
    placeholder?: string;
    onselect: (value: string, item: ListboxItem<T>) => void;
    class?: string;
    buttonClass?: string;
    dropdownClass?: string;
    ariaLabel?: string;
    trigger?: Snippet<[{ item: ListboxItem<T> | null; isOpen: boolean }]>;
    option?: Snippet<[{ item: ListboxItem<T>; isSelected: boolean; isFocused: boolean }]>;
  }

  let {
    id = "listbox",
    value,
    items,
    groups = [],
    placeholder = "",
    onselect,
    class: className = "",
    buttonClass = "",
    dropdownClass = "",
    ariaLabel,
    trigger,
    option,
  }: Props<T> = $props();

  let isOpen = $state(false);
  let containerEl = $state<HTMLDivElement | null>(null);
  let buttonEl = $state<HTMLButtonElement | null>(null);
  let listboxEl = $state<HTMLUListElement | null>(null);
  let focusedIndex = $state<number>(-1);

  let selectableItems = $derived(items.filter((item) => !item.disabled));
  let selectedItem = $derived(items.find((item) => item.value === value) ?? null);

  let activeDescendant = $derived(
    isOpen && focusedIndex >= 0 && focusedIndex < selectableItems.length
      ? selectableItems[focusedIndex].id
      : undefined
  );

  function openDropdown() {
    if (isOpen) return;
    isOpen = true;
    const currentIdx = selectableItems.findIndex((item) => item.value === value);
    focusedIndex = currentIdx >= 0 ? currentIdx : 0;
    tick().then(() => {
      scrollToFocused();
    });
  }

  function closeDropdown(refocus = true) {
    if (!isOpen) return;
    isOpen = false;
    focusedIndex = -1;
    if (refocus && buttonEl) {
      buttonEl.focus();
    }
  }

  function toggleDropdown() {
    if (isOpen) {
      closeDropdown();
    } else {
      openDropdown();
    }
  }

  function selectItem(item: ListboxItem<T>) {
    if (item.disabled) return;
    onselect(item.value, item);
    closeDropdown();
  }

  function scrollToFocused() {
    if (!listboxEl || focusedIndex < 0) return;
    const item = selectableItems[focusedIndex];
    if (!item) return;
    const el = listboxEl.querySelector<HTMLElement>(`#${item.id}`);
    if (el && typeof el.scrollIntoView === "function") {
      el.scrollIntoView({ block: "nearest" });
    }
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === "Tab") {
      if (isOpen) closeDropdown(false);
      return;
    }

    if (!isOpen) {
      if (e.key === "ArrowDown" || e.key === "ArrowUp" || e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        openDropdown();
      }
      return;
    }

    switch (e.key) {
      case "ArrowDown": {
        e.preventDefault();
        if (selectableItems.length > 0) {
          focusedIndex = (focusedIndex + 1) % selectableItems.length;
          scrollToFocused();
        }
        break;
      }
      case "ArrowUp": {
        e.preventDefault();
        if (selectableItems.length > 0) {
          focusedIndex = (focusedIndex - 1 + selectableItems.length) % selectableItems.length;
          scrollToFocused();
        }
        break;
      }
      case "Home": {
        e.preventDefault();
        focusedIndex = 0;
        scrollToFocused();
        break;
      }
      case "End": {
        e.preventDefault();
        focusedIndex = Math.max(0, selectableItems.length - 1);
        scrollToFocused();
        break;
      }
      case "Enter":
      case " ": {
        e.preventDefault();
        if (focusedIndex >= 0 && focusedIndex < selectableItems.length) {
          selectItem(selectableItems[focusedIndex]);
        }
        break;
      }
      case "Escape": {
        e.preventDefault();
        closeDropdown();
        break;
      }
      default: {
        if (
          e.key.length === 1 &&
          !e.ctrlKey &&
          !e.metaKey &&
          !e.altKey &&
          selectableItems.length > 0
        ) {
          const char = e.key.toLowerCase();
          const start = (focusedIndex + 1) % selectableItems.length;
          const matchIdx = [
            ...selectableItems.slice(start),
            ...selectableItems.slice(0, start),
          ].findIndex((item) => item.label.trim().toLowerCase().startsWith(char));
          if (matchIdx !== -1) {
            focusedIndex = (start + matchIdx) % selectableItems.length;
            scrollToFocused();
          }
        }
        break;
      }
    }
  }

  function handleWindowPointerDown(e: PointerEvent) {
    if (isOpen && containerEl && !containerEl.contains(e.target as Node)) {
      closeDropdown(false);
    }
  }

  onMount(() => {
    window.addEventListener("pointerdown", handleWindowPointerDown);
    return () => {
      window.removeEventListener("pointerdown", handleWindowPointerDown);
    };
  });
</script>

<div bind:this={containerEl} class="relative inline-block {className}">
  <!-- Combobox trigger -->
  <button
    bind:this={buttonEl}
    {id}
    type="button"
    role="combobox"
    aria-haspopup="listbox"
    aria-expanded={isOpen}
    aria-controls={`${id}-listbox`}
    aria-activedescendant={activeDescendant}
    aria-label={ariaLabel}
    onclick={toggleDropdown}
    onkeydown={handleKeyDown}
    class="bg-brand-main text-xs text-brand-text-primary border border-brand-border rounded pl-3.5 pr-2 py-1 outline-none focus:border-brand-accent font-medium flex items-center justify-between gap-2.5 cursor-pointer hover:border-brand-accent/50 transition-colors select-none min-w-[9.5rem] {buttonClass}"
  >
    <div class="flex items-center gap-2 min-w-0">
      {#if trigger}
        {@render trigger({ item: selectedItem, isOpen })}
      {:else}
        <span class="truncate">
          {selectedItem ? selectedItem.label : placeholder}
        </span>
      {/if}
    </div>
    <CaretDown
      class="w-3 h-3 text-brand-text-secondary shrink-0 transition-transform duration-150 {isOpen ? 'rotate-180' : ''}"
    />
  </button>

  {#if isOpen}
    <div
      class="absolute left-0 top-full mt-1.5 z-50 min-w-[13.5rem] max-h-72 overflow-y-auto bg-brand-sidebar border border-brand-border rounded-xl shadow-2xl py-1.5 focus:outline-none {dropdownClass}"
    >
      <ul
        bind:this={listboxEl}
        id={`${id}-listbox`}
        role="listbox"
        tabindex="-1"
        aria-label={ariaLabel ?? placeholder}
        class="focus:outline-none"
      >
        {#if groups.length > 0}
          {#each groups as group, groupIdx (group.id)}
            {@const groupItems = items.filter((item) => item.group === group.id)}
            {#if groupItems.length > 0}
              <li
                role="presentation"
                class="pt-1 {groupIdx > 0 ? 'border-t border-brand-border/40 mt-1' : ''}"
              >
                <div
                  role="presentation"
                  class="px-3 py-1 text-[10px] font-semibold tracking-wider uppercase text-brand-text-secondary/70"
                >
                  {group.label}
                </div>
                <ul role="group" aria-label={group.label}>
                  {#each groupItems as item (item.id)}
                    {@const selectableIdx = selectableItems.indexOf(item)}
                    {@const isFocused = focusedIndex === selectableIdx}
                    {@const isSelected = value === item.value}
                    <!-- svelte-ignore a11y_click_events_have_key_events -->
                    <li
                      id={item.id}
                      role="option"
                      tabindex="-1"
                      aria-selected={isSelected}
                      aria-disabled={item.disabled}
                      class="px-3 py-1.5 text-xs flex items-center justify-between gap-3 transition-colors {item.disabled ? 'text-brand-text-secondary/50 italic cursor-not-allowed' : 'cursor-pointer'} {isFocused && !item.disabled ? 'bg-brand-accent/20 text-brand-text-primary' : isSelected ? 'bg-brand-main text-brand-accent-text font-medium' : !item.disabled ? 'text-brand-text-primary hover:bg-brand-main/80' : ''}"
                      onclick={() => selectItem(item)}
                      onkeydown={(e) => {
                        if (e.key === "Enter" || e.key === " ") {
                          e.preventDefault();
                          selectItem(item);
                        }
                      }}
                      onmouseenter={() => {
                        if (!item.disabled) focusedIndex = selectableIdx;
                      }}
                    >
                      {#if option}
                        {@render option({ item, isSelected, isFocused })}
                      {:else}
                        <span class="truncate">{item.label}</span>
                      {/if}
                    </li>
                  {/each}
                </ul>
              </li>
            {/if}
          {/each}
        {:else}
          {#each items as item (item.id)}
            {@const selectableIdx = selectableItems.indexOf(item)}
            {@const isFocused = focusedIndex === selectableIdx}
            {@const isSelected = value === item.value}
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <li
              id={item.id}
              role="option"
              tabindex="-1"
              aria-selected={isSelected}
              aria-disabled={item.disabled}
              class="px-3 py-1.5 text-xs flex items-center justify-between gap-3 transition-colors {item.disabled ? 'text-brand-text-secondary/50 italic cursor-not-allowed' : 'cursor-pointer'} {isFocused && !item.disabled ? 'bg-brand-accent/20 text-brand-text-primary' : isSelected ? 'bg-brand-main text-brand-accent-text font-medium' : !item.disabled ? 'text-brand-text-primary hover:bg-brand-main/80' : ''}"
              onclick={() => selectItem(item)}
              onkeydown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  selectItem(item);
                }
              }}
              onmouseenter={() => {
                if (!item.disabled) focusedIndex = selectableIdx;
              }}
            >
              {#if option}
                {@render option({ item, isSelected, isFocused })}
              {:else}
                <span class="truncate">{item.label}</span>
              {/if}
            </li>
          {/each}
        {/if}
      </ul>
    </div>
  {/if}
</div>
