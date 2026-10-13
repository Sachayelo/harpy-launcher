<script lang="ts">
  import { tick } from 'svelte'
  import { CHANGE_KINDS, type Change } from './api'

  // A patch note, one line per change. The pill sets the category (click to
  // change it); Enter adds a line, Backspace on an empty line removes it, and
  // pasting several lines makes as many changes.
  let { changes = $bindable(), placeholder }: { changes: Change[]; placeholder: string } = $props()

  let inputs: HTMLInputElement[] = $state([])

  const label = (kind: Change['kind']) => CHANGE_KINDS.find((item) => item.kind === kind)?.label ?? 'Nouveau'

  function cycle(index: number) {
    const order = CHANGE_KINDS.map((item) => item.kind)
    const next = order[(order.indexOf(changes[index].kind) + 1) % order.length]
    changes[index] = { ...changes[index], kind: next }
  }

  async function focus(index: number) {
    await tick()
    inputs[index]?.focus()
  }

  function add(after = changes.length - 1) {
    const kind = changes[after]?.kind ?? 'new'
    changes.splice(after + 1, 0, { kind, text: '' })
    focus(after + 1)
  }

  function remove(index: number) {
    changes.splice(index, 1)
    focus(Math.max(0, index - 1))
  }

  function key(event: KeyboardEvent, index: number) {
    if (event.key === 'Enter') {
      event.preventDefault()
      add(index)
    } else if (event.key === 'Backspace' && !changes[index].text && changes.length > 1) {
      event.preventDefault()
      remove(index)
    }
  }

  function paste(event: ClipboardEvent, index: number) {
    const text = event.clipboardData?.getData('text') ?? ''
    const lines = text
      .split(/\r?\n/)
      .map((line) => line.replace(/^\s*[-*•]\s*/, '').trim())
      .filter(Boolean)
    if (lines.length < 2) return
    event.preventDefault()
    const kind = changes[index].kind
    const fresh = lines.map((line) => ({ kind, text: line }))
    if (changes[index].text.trim()) changes.splice(index + 1, 0, ...fresh)
    else changes.splice(index, 1, ...fresh)
  }
</script>

<div class="editor">
  {#each changes as change, i (i)}
    <div class="row">
      <button class="kind {change.kind}" title="Changer de catégorie" onclick={() => cycle(i)}>{label(change.kind)}</button>
      <input
        bind:this={inputs[i]}
        bind:value={change.text}
        placeholder={i === 0 ? placeholder : ''}
        onkeydown={(event) => key(event, i)}
        onpaste={(event) => paste(event, i)}
      />
      <button class="remove" aria-label="Retirer" onclick={() => remove(i)}>
        <svg viewBox="0 0 10 10"><path d="M2 2l6 6M8 2l-6 6" /></svg>
      </button>
    </div>
  {/each}
  <button class="add" onclick={() => add()}>+ Ajouter une ligne</button>
</div>

<style>
  .editor {
    display: grid;
    gap: 5px;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .kind {
    --tint: var(--brass);
    flex: none;
    width: 86px;
    padding: 4px 0;
    border: 1px solid color-mix(in srgb, var(--tint) 40%, transparent);
    border-radius: 999px;
    background: color-mix(in srgb, var(--tint) 10%, transparent);
    font-size: 11px;
    font-weight: 600;
    color: var(--tint);
    cursor: pointer;
  }

  .kind.balance {
    --tint: #8cb8ef;
  }

  .kind.fix {
    --tint: var(--ok);
  }

  input {
    flex: 1;
    min-width: 0;
    padding: 6px 9px;
    border: 1px solid var(--line);
    border-radius: 7px;
    background: #0a0e16;
    font: 13px var(--sans);
    color: var(--text);
    user-select: text;
  }

  input:focus {
    outline: none;
    border-color: rgba(233, 168, 98, 0.45);
  }

  .remove {
    flex: none;
    width: 24px;
    height: 24px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--muted);
    cursor: pointer;
  }

  .remove:hover {
    color: var(--warn);
  }

  .remove svg {
    width: 9px;
    height: 9px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.4;
  }

  .add {
    justify-self: start;
    padding: 2px 0;
    border: 0;
    background: none;
    font-size: 12px;
    color: var(--muted);
    cursor: pointer;
  }

  .add:hover {
    color: var(--brass);
  }
</style>
