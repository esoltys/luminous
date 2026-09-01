# Vendored generators

`flatpak-cargo-generator.py` is vendored verbatim (unmodified) from
[flatpak/flatpak-builder-tools](https://github.com/flatpak/flatpak-builder-tools),
pinned at commit
[`f03a673`](https://github.com/flatpak/flatpak-builder-tools/commit/f03a673abe6ce189cea1c2857e2b44af2dd79d1f)
(`cargo/flatpak-cargo-generator.py`), MIT licensed. It's run via `uv run` (see
`scripts/sync-flatpak.ts`), which reads its inline PEP 723 dependency block
and manages its own venv — no separate `pip install` step needed.

`flatpak-node-generator` isn't vendored the same way (it's a proper Python
package, not a single script) — `scripts/sync-flatpak.ts` instead runs it via
`uvx --from git+https://github.com/flatpak/flatpak-builder-tools@<pinned-sha>#subdirectory=node`,
pinned to a specific commit of the same upstream repo's `node/` directory.

To update either pin: pick a new commit from
https://github.com/flatpak/flatpak-builder-tools, re-copy
`cargo/flatpak-cargo-generator.py` here verbatim for the cargo generator, and
update `NODE_GENERATOR_REF` in `scripts/sync-flatpak.ts` for the node
generator. Then run `bun run scripts/sync-flatpak.ts` and review the diff —
a generator update can change output shape even without any of our own
dependencies changing.
