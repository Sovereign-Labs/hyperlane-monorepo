# Repo Guide (AI)

This is a large monorepo. When working on codebase, only study the codebase necessary to prevent context overflow.
Spawn subagents whenever possible to offload context and preserve top-level context with necessary items.

Workspace layout:
- Binaries live in `bins/*`.
- Libraries and shared assets live in `libs/*`.

Per-package guidance lives in `bins/<name>/AGENTS.md` or `libs/<name>/AGENTS.md`.
Current package guides:
- `bins/price-crank/` -> `bins/price-crank/AGENTS.md`
- `bins/liquidator/` -> `bins/liquidator/AGENTS.md`
- `bins/websocket-server/` -> `bins/websocket-server/AGENTS.md`

Commands:
```bash
just
just build
just build-release
just run BIN=<bin>
just run-debug BIN=<bin>
just test
just test-all
just test-full-local
just ci
just docker-build <bin>
```

Rules:
- Always run `just ci` before finishing work and report results.
- Enforce highest code quality: follow existing patterns, keep formatting/clippy clean, add/update tests and docs when behavior changes.
- Preserve tokens: be concise, read only what you need, avoid dumping large files or repeated output.
- Never make any assumptions or guesses. Do your research including looking things up on the internet and ask the user for confirmation if that doesn't work.
