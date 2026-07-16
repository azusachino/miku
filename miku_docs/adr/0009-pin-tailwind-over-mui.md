# 9. Pinning Tailwind CSS for Server-Rendered HTML (Deciding Against MUI)

- **Status**: Accepted
- **Date**: 2026-06-27
- **Author**: Haru & Antigravity

## Context

Miku Wiki requires rich aesthetics, modern layouts, and premium visuals matching applications like Notion and Obsidian.

During layout reviews, the option of switching the frontend stylesheet framework from Tailwind CSS to React-centric libraries like **MUI (Material UI)** was considered. However, MUI requires React's
virtual DOM, hydration engine, and a CSS-in-JS compiler (such as Emotion or styled-components). Utilizing React/MUI would require migrating Miku from a simple Rust-based Multi-Page Application (MPA)
into a client-side Single Page Application (SPA), introducing a complex Javascript bundler pipeline (Vite, Webpack, or Next.js) and Node.js environment requirements.

## Decision

We pin **Tailwind CSS** as Miku's layout and styling framework, rejecting React-dependent component systems like MUI.

Tailwind is chosen because:

1. **Zero-Bundler Alignment**: Tailwind works perfectly by serving inline classes directly inside our Rust-driven MiniJinja HTML templates (`base.html`, `page.html`, `edit.html`). It does not require
   a Javascript compilation pipeline during deployment or execution.
2. **Performance & Lightweight Footprint**: Tailwind produces static CSS (either via Play CDN or static compilation). It runs with zero JS execution overhead in the browser, avoiding the DOM hydration
   penalty and runtime weight of React.
3. **Material Design Customization**: If Material Design aesthetics are desired, they can be accomplished via custom Tailwind configuration classes or lightweight CSS frameworks (like Beer CSS or MDC
   Web) rather than importing React.

## Consequences

- **Template-First Development**: All layout changes, responsive grids, and design tokens will be styled using Tailwind utility classes directly in the HTML template files.
- **Preservation of Rust MPA Design**: The application maintains its zero-compilation build pipeline—requiring only `cargo build` and no `npm run build` steps.
- **Consistent CSS Rules**: We will actively migrate remaining legacy CSS custom styles in `miku.css` into Tailwind utility components, preserving the custom variables theme system for dark/light
  accent control.
