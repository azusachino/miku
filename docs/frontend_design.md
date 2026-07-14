# Miku Frontend Architecture & Communication Plan

This document details how the frontend communicates with the backend in a **zero-bundler, server-rendered** environment, and how it integrates **Alpine.js**, **Prism.js**, and **Mermaid.js**.

---

## 1. The Stack: Alpine.js + Vendor Scripts

We use **Alpine.js** to handle interactive behaviors (dropdowns, modals, autocompletes) and coordinate third-party libraries (Prism for syntax highlighting, Mermaid for diagrams) using Alpine's
lifecycle events.

- **Alpine.js**: Handles local DOM states (autocomplete list, search modal, dropdown triggers).
- **Prism.js**: Handles client-side code block syntax highlighting.
- **Mermaid.js**: Handles rendering flowchart/diagram syntax inside markdown pages.
- **Delivery:** Scripts are loaded via normal `<script>` tags (served locally from `/static/js/vendor/` to allow offline operation).

---

## 2. Code Highlighting Integration (Prism.js)

We run **Prism.js** in the browser to highlight code blocks. To ensure formatting runs on page load and after dynamic page swaps, we trigger it via Alpine's `x-init` lifecycle directive:

```html
<!-- Main page layout shell -->
<div x-data x-init="Prism.highlightAll()" class="content-container">
  <!-- Server-rendered Markdown HTML -->
  <pre><code class="language-rust">
    fn main() {
        println!("Hello Miku!");
    }
  </code></pre>
</div>
```

- **Why this works:** Alpine's `x-init` runs immediately after the element is loaded into the DOM, calling Prism to run its regex-tokenization on code blocks and inject style classes.

---

## 3. Lazy-Loaded Diagram Rendering (Mermaid.js)

`mermaid.js` is a heavy library ($>1\text{MB}$). Loading it on every page slows down render performance. We optimize this using a **selective loading and lazy initialization** pattern:

### Step A: The DB Flag

During background markdown parsing, the indexer checks if the page contains a ` ```mermaid ` block. If it does, it sets `has_mermaid = true` in the database (`tb_pages`).

### Step B: Conditional Server Injection

In the Miku server template (`src/templates/`), we conditionally inject the script tags only if the page metadata indicates `has_mermaid` is true:

```html
<!-- In page_view.html template -->
{% if page.has_mermaid %}
<!-- Load Mermaid locally -->
<script src="/static/js/vendor/mermaid.min.js"></script>
<script>
  document.addEventListener("DOMContentLoaded", () => {
    mermaid.initialize({ startOnLoad: true, theme: "default" });
  });
</script>
{% endif %}
```

### Step C: Alpine-Driven Async Initializer (Optional for dynamic swaps)

If pages are swapped dynamically (e.g. via fetch/navigation without full reload), we use Alpine to trigger Mermaid rendering on-demand:

```html
<div x-data x-init="if (window.mermaid) { mermaid.run() }" class="markdown-body">
  <!-- Rendered comrak output containing <pre class='mermaid'> blocks -->
</div>
```

---

## 4. Editor Autocomplete & Command Palette (Alpine.js)

Alpine handles local state, communicating with the Axum JSON API:

```html
<!-- Ctrl-K Command Palette Modal -->
<div
  x-data="{ open: false, query: '', results: [] }"
  @keydown.window.cmd.k.prevent="open = !open; $nextTick(() => $refs.searchInput.focus())"
  @keydown.window.ctrl.k.prevent="open = !open; $nextTick(() => $refs.searchInput.focus())"
  x-show="open"
  class="modal-backdrop"
  style="display: none;"
>
  <div class="modal-content" @click.away="open = false">
    <input
      x-ref="searchInput"
      x-model="query"
      @input.debounce.150ms="
        const res = await fetch(`/api/v1/autocomplete?q=${encodeURIComponent(query)}`);
        results = await res.json();
      "
      type="text"
      placeholder="Search or jump to page..."
      class="palette-input"
    />

    <ul class="palette-results">
      <template x-for="item in results">
        <li>
          <a :href="'/p/' + item.slug" x-text="item.title"></a>
        </li>
      </template>
    </ul>
  </div>
</div>
```
