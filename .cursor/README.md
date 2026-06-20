# Cursor project configuration

This folder holds **persistent AI guidance** for Kobayashi. It is separate from [CLAUDE.md](../CLAUDE.md), which documents commands, architecture, and data layout for humans and agents alike.

## Layout

| Path | Purpose |
|------|---------|
| `rules/*.mdc` | Numbered Cursor rules (YAML frontmatter + markdown body). |
| `plans/` | Ephemeral Cursor Plan artifacts. **Not maintained** after the work ships — delete or ignore stale plans here. |

There is no project `AGENTS.md`; agent expectations live in `rules/` and `CLAUDE.md`.

## Rule numbering

| Prefix | Scope |
|--------|--------|
| `00–09` | Always-on project principles |
| `10–19` | Rust simulator / optimizer (`src/`) |
| `20–29` | Game data and LCARS |
| `30–39` | Tests, fixtures, evaluation discipline (`31-benchmark-fixtures.mdc` → `tests/fixtures/`; `32-cargo-test-invocation.mdc` → always-on terminal `cargo test` targets) |
| `40–49` | Importers, scripts, normalization |
| `50–59` | Combat mechanics (narrow) |
| `60–69` | Frontend SPA |

When adding a rule: one concern per file, under ~50 lines, actionable examples where helpful. Prefer `globs` over `alwaysApply: true` unless the guidance applies to every task.

## Maintenance checklist

- **After a Plan completes:** remove or archive the plan file; fold durable facts into `docs/`, `data/*/README.md`, or a rule if agents keep repeating the same mistake.
- **When CLAUDE.md changes** (new commands, modules, env vars): check whether any rule duplicates or contradicts it; rules should encode *behavior*, not duplicate the full command reference.
- **Dead globs:** if a rule's `globs` pattern matches no files, update or delete the rule (e.g. fixture paths under `tests/fixtures/`).
- **Do not commit** secrets, local profile paths, or machine-specific settings under `.cursor/`.

## Related docs

- [CLAUDE.md](../CLAUDE.md) — build, test, architecture
- [CONTRIBUTING.md](../CONTRIBUTING.md) — CI and hooks
- [docs/LCARS_CONTRIBUTING.md](../docs/LCARS_CONTRIBUTING.md) — officer ability authoring
