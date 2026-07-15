# Contributing to Miku Note

Thanks for helping improve Miku Note.

## Before you start

For larger changes, open an issue first so the intended behavior and scope are clear. Small documentation, test, and bug-fix pull requests can go directly to a branch.

Please do not include private vault content, generated runtime indexes, credentials, screenshots containing personal data, or unrelated formatting changes.

## Local workflow

```bash
nix develop
make check
```

The default check covers Rust formatting, generated CSS, Prettier, Ruff, Python tests, Clippy, and the Rust workspace tests. For behavior changes, also run the narrowest relevant target:

```bash
make check-blackbox
make check-ux-browser
make check-all-features
```

The browser check may need a one-time `uv run playwright install chromium`.

## Pull requests

- Use a focused branch such as `feat/...`, `fix/...`, or `docs/...`.
- Explain the user-visible behavior and the implementation boundary.
- Add or update tests for behavior changes.
- Update documentation and the changelog when the public behavior changes.
- Keep commits small and use conventional prefixes such as `feat:`, `fix:`, `docs:`, or `test:`.
- Include the exact checks you ran in the pull request description.

## Design expectations

The Markdown vault is the source of truth. Derived indexes must remain rebuildable, page reads should not depend on a complete background reconcile, and the read path should stay lightweight. Prefer
existing workspace crates and modules over adding a new abstraction boundary without a clear need.

## License

By contributing, you agree that your contribution is provided under the repository's [GPL-3.0-or-later license](LICENSE).
