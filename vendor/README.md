# Reference projects

These repositories are vendored as shallow Git submodules for UX and
architecture study only. They are not workspace members, build inputs, or
runtime dependencies of Miku.

| Project | Submodule | License | Primary study target |
| --- | --- | --- | --- |
| Tolaria | `vendor/tolaria` | AGPL-3.0 | files-first vault UX, keyboard workflow, relationships, frontmatter |
| Trilium | `vendor/trilium` | AGPL-3.0 | hierarchical tree, workspace, note navigation, hoisting, context actions |
| SilverBullet | `vendor/silverbullet` | MIT | browser Markdown workspace, page picker, links, programmable boundaries |

The AGPL repositories are reference material. Do not copy implementation code,
assets, or derived source into Miku without a separate license review. Product
ideas and interaction observations can be reimplemented independently.

Useful inspection commands:

```bash
git submodule update --init --depth 1
rg -n "tree|workspace|hoist|bookmark|navigation|page picker|backlink" vendor/trilium vendor/tolaria vendor/silverbullet
git -C vendor/trilium log --oneline -20
git -C vendor/tolaria log --oneline -20
git -C vendor/silverbullet log --oneline -20
```

Keep reference changes isolated from Miku's product code and exclude all three
paths from Rust, frontend, packaging, and release build inputs.
