# miku-web

The replaceable React workspace for Miku 0.0.3. This package currently runs
against fixture data; task-5 will connect the query boundary to Rust's
`/api/v1/*` contract.

```sh
bun install
bun run dev
bun run check
```

The shell intentionally owns only ephemeral presentation state: tabs, split
panes, focus, context visibility, and hoisting. Note content, placements, and
revisions stay on the API boundary.
