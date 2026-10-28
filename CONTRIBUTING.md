# Contributing to Windmill

Thanks for helping make the terminal a better place for music.

## Before you start

Review the [README](README.md), [developer guide](docs/DEVELOPMENT.md), and [roadmap](docs/ROADMAP.md). For large features or behavior changes, open a GitHub issue first so work doesn't duplicate ongoing efforts. Small typo fixes and self-contained bug fixes can go straight to a PR.

## Workflow

1. Fork the repository, branch from the current main, and keep your changes focused.
2. For code changes, add a regression test or explain why an automated test is impractical.
3. Run the checks from the [developer guide](docs/DEVELOPMENT.md): Rust fmt/test/clippy/build and relevant integration tests.
4. Document changed user-facing behavior, shortcuts, configuration, and limitations.
5. Open a pull request describing what changed, how you verified it, and any terminal/system compatibility concerns.

## Scope and safety

- Do not commit copyrighted music, downloaded videos, user media caches, real SQLite libraries, personal file paths, credentials, cookies, or private URLs.
- Do not send user library contents to analytics or third-party services. Online provider requests should be explicit and documented.
- Never require elevated privileges to run the Rust app or its tests.
- Use test fixtures, isolated `WINDMILL_DATA_DIR` profiles, silent mpv playback, and mocked API responses rather than touching user music.
- Keep performance in mind: don't block the TUI loop or repeatedly decode artwork every frame.
- Be respectful of third-party copyright, service terms, and existing project licenses.

## Reporting bugs

Include Windmill version/commit, OS and terminal, reproduction steps, expected vs actual behavior, and a relevant sanitized log or screenshot. Redact personal media names, local paths, access tokens, and private playlist links.

## Code of conduct

Be constructive and welcoming. Critique code, not people; harassment, discrimination and personal attacks are not welcome.

By contributing, you agree your contributions are licensed under the project's MIT License.
