# AGENTS.md

Operating guide for AI agents and humans working in this repository.

## Project

`zalo-rs` is a native Zalo desktop client written in Rust, talking directly to Zalo's Web/PC APIs. No Node.js, no Electron, no Chromium.

- Repository: <https://github.com/lgthevinh/zalo-rs> (fork of <https://github.com/ctdoteam1110/ctdozlite>)
- Rust 1.85+ / edition 2024
- Docs are English: Zalo reverse-engineering specs in `docs/zalo-specs/`, contributor rules in `.github/CONTRIBUTING.md`

## Status: rewrite in progress

The project is being rewritten from scratch. The previous implementation is archived on the `legacy` branch; do not port code from it blindly.

- **`docs/zalo-specs/` is the source of truth** for Zalo's behaviour. Start from its `README.md`.
- Claims tagged `[I]` came from the previous client without live evidence, and `[U]` are open questions. Verify them (see `docs/zalo-specs/tooling/capture.md`) before relying on them.
- Each spec's *Open questions* list also records known bugs in the previous client. Do not reproduce them.

## Commands

| Task | Command |
| :--- | :--- |
| Build | `cargo build --locked` |
| Test | `cargo test --locked` |
| Format | `cargo fmt --all` |
| Lint | `cargo clippy --all-targets -- -D warnings` |

On Debian/Ubuntu, audio support needs `libasound2-dev`. Set `CMAKE_POLICY_VERSION_MINIMUM=3.5` if a dependency builds with CMake 4.x.

## Non-Negotiables

- **Never commit secrets.** `session.json`, `*.db`, cookies, `zpw_sek`, `zpw_enk`, IMEI and capture dumps stay out of Git (see `.gitignore`). Zalo's static bundle constants in `docs/zalo-specs/security/param-cipher.md` are protocol facts, not secrets.
- **No blocking work on the UI thread.** Network, disk and crypto run off the UI thread; the UI only renders state and sends commands.
- **One source for hosts.** Hosts come from the server's service map (`docs/zalo-specs/auth/service-discovery.md`); fallback defaults live in a single module. Never hardcode a Zalo host elsewhere.
- **Encode each request the way its endpoint group requires.** Before login and for the login group, use `ParamCipher v2` plus `signkey`. After login, data calls encrypt `params` with the session key `zpw_enk` and send no `signkey`. All requests carry the common params, headers and session cookie. See `docs/zalo-specs/security/` and `docs/zalo-specs/auth/session.md`.
- **Comments in English, user-facing strings in Vietnamese.** See *Code comments* in `.github/CONTRIBUTING.md`.
- **Do not silently change protocol behaviour.** Any change to params, signing, frame IDs or cipher flow requires evidence (a capture, recorded in `docs/zalo-specs/` with an `[O]` tag) and a test or documented manual verification.

## Engineering Standards

MUST read the matching section of `.github/CONTRIBUTING.md` before the related work:

- **Issue tracking & tickets** → *Reporting issues*
- **Comments & docstrings** → *Code comments*
- **Branches, commits, and PRs** → *Branches*, *Commit messages*, *Pull requests*
