# Cursor project instructions

[AGENTS.md](../AGENTS.md) is the shared instruction source for this repository.
Cursor reads the root file; `rules/00-global-kobayashi.mdc` is a compatibility
entry point that directs the agent to it. Claude uses the same source through
the import in [CLAUDE.md](../CLAUDE.md).

The former scoped Cursor rules have been consolidated into `AGENTS.md`:

| Former topic | Shared section |
| --- | --- |
| Global goals and checklist maintenance | Instruction maintenance; Protect player state and existing work |
| Rust architecture | Project map |
| Frontend | Frontend and API contracts |
| Game data, LCARS and importers | Data and officer definitions |
| Combat mechanics, benchmarks and fixtures | Combat correctness and evidence |
| Cargo invocation and terminal discipline | Rust commands and test discipline |

## Maintenance

- Change shared policy in `AGENTS.md`, not in parallel Claude or Cursor copies.
- Add a scoped rule only for a requirement that cannot be expressed adequately in the shared instructions. Keep it short and check its globs against real paths.
- Cursor rules must use `.mdc` frontmatter. References point the agent to a file; they do not inline its contents.
- `plans/` contains temporary planning artifacts. Move durable conclusions into maintained project documentation when work completes.
- Keep local permissions, machine paths, secrets and session status out of shared rules. Tool settings do not create authorization for unrelated actions.

Adapter formats follow the official [Cursor rules](https://cursor.com/docs/rules)
and [Claude memory/import](https://code.claude.com/docs/en/memory#share-one-file-with-other-coding-tools)
documentation. The project policy remains in `AGENTS.md`.
