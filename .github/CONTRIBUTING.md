# Contributing to zalo-rs

Thanks for helping out. This guide covers how we track work, write code and ship changes. Read the section that matches what you are about to do.

## Contents

- [Ground rules](#ground-rules)
- [Reporting issues](#reporting-issues)
- [Development setup](#development-setup)
- [Branches](#branches)
- [Commit messages](#commit-messages)
- [Pull requests](#pull-requests)
- [Code comments](#code-comments)
- [Releases](#releases)

## Ground rules

- **Never share secrets.** `session.json`, `*.db`, cookies, `zpw_sek`, `zpw_enk`, IMEI and raw captures stay out of Git, issues and PRs. Redact them as `zpw_sek=<redacted>`.
- **Protocol changes need evidence.** Any change to params, signing, frame IDs or the cipher flow needs a capture or a note in [`docs/zalo-specs/`](../docs/zalo-specs/README.md), plus a test or written manual verification.
- **Humans approve commits.** Never run `git commit` or `git push` on someone's behalf without their explicit go-ahead.

## Reporting issues

Issues live in GitHub Issues. Issue state is tracked there; notes in `docs/zalo-specs/` are evidence, never tickets.

### Before you open one

Search open and closed issues first. If yours is new, include:

- [ ] A one-sentence outcome (observable behaviour, not a file list).
- [ ] Steps to reproduce, or an acceptance checklist.
- [ ] OS and app version.
- [ ] For protocol issues: capture path, `docs/zalo-specs/` reference, server `error_code`.
- [ ] Explicit non-goals when the scope is easy to over-read.

### Working with `gh`

| Task | Command |
| :--- | :--- |
| Create | `gh issue create --title "..." --body "..."` (use a heredoc for long bodies) |
| View | `gh issue view <number> --comments` |
| List | `gh issue list --state open --label "<label>"` |
| Comment | `gh issue comment <number> --body "..."` |
| Label | `gh issue edit <number> --add-label "<label>"` |
| Close | `gh issue close <number> --comment "..."` (always with a resolution summary) |

### Labels

State comes from open/closed, PR links (`Fixes #<id>`) and GitHub Projects. Do not add `in-progress` or `needs-review` labels.

| Group | Labels |
| :--- | :--- |
| Status | `status:blocked` (waiting on a dependency, credentials, protocol evidence or another issue), `wontfix` |
| Type | `type:feat`, `type:fix`, `type:refactor`, `type:perf`, `type:test`, `type:docs`, `type:chore`, `type:ci`, `type:style`, `type:task` |
| Priority | `priority:p0` (data loss, credential exposure, outage), `priority:p1` (blocks next release), `priority:p2` (default) |

| Area | Scope |
| :--- | :--- |
| `area:auth` | QR/password login, `session.json`, `zpw_sek` / `zpw_enk` |
| `area:net` | HTTP endpoints, WebSocket transport, `ParamCipher v2`, `signkey` |
| `area:chat` | Threads, messages, reactions, recall, forwarding |
| `area:media` | Attachments, images, stickers, voice payloads |
| `area:call` | VoIP signalling, audio/video capture |
| `area:ui` | egui GUI, ratatui TUI, theme, fonts |
| `area:store` | SQLite schema, queries, local history |
| `area:ci` | Build tooling and release artifacts |
| `area:harness` | `AGENTS.md`, this guide, agent tooling |

### Triage

These terms are used by `/triage`. Do not introduce synonyms.

| Term | Meaning in the repo |
| :--- | :--- |
| `needs-triage` | Open issue with no `type:*` or `area:*` label. |
| `needs-info` | `status:blocked` plus a comment naming the missing evidence. |
| `ready-for-agent` | `type:*` and `area:*` set, acceptance criteria present, no open blocker. |
| `ready-for-human` | `priority:p0`/`p1`, or needs protocol evidence or a physical device. |
| `wontfix` | `wontfix` label plus a closing comment. |

### Sub-issues

```bash
# 1. Create the child
gh issue create --title "task(<area>): <summary>" --body "..." --label "type:task,area:<area>"

# 2. Link it to the parent
CHILD_ID=$(gh api repos/{owner}/{repo}/issues/<child_number> --jq .id)
gh api --method POST repos/{owner}/{repo}/issues/<parent_number>/sub_issues -F sub_issue_id=$CHILD_ID

# 3. List children
gh api repos/{owner}/{repo}/issues/<parent_number>/sub_issues --jq '.[] | "#\(.number): \(.title)"'
```

For larger efforts, use one map ticket labeled `wayfinder:map` (decisions and open questions) with children labeled `wayfinder:<research|prototype|grilling|task>`. Use native issue dependencies for blocking; a ticket is unblocked when every blocker is closed.

## Development setup

Requires Rust 1.85+ (edition 2024). Logging is controlled with `RUST_LOG` (default `info`).

| Task | Command |
| :--- | :--- |
| Build | `cargo build --locked` |
| Test | `cargo test --locked` |
| Format | `cargo fmt --all` |
| Lint | `cargo clippy --all-targets -- -D warnings` |
| Run | `cargo run` |

## Branches

| Prefix | Use |
| :--- | :--- |
| `feature/<kebab-name>` | New capability |
| `fix/<kebab-name>` | Bug fix |
| `hotfix/<kebab-name>` | Production patch, branched from the release tag |
| `core/<kebab-name>` | Architecture, schema, protocol or crypto refactor |
| `docs/<kebab-name>` | Documentation only |
| `chore/<kebab-name>` | Tooling, dependencies, CI |

- Branch from `main` and keep branches short-lived.
- One logical change per branch. Do not mix formatting sweeps with behaviour changes.
- Rebase on `main` before opening the PR; no merge commits inside a feature branch.

## Commit messages

We follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <summary>

- why this change is needed
- any notable consequence
```

- Header at most 72 characters, imperative mood, no trailing period.
- Body explains **why**, as short `-` bullets, not a file list.
- One commit is one atomic, buildable change.
- Breaking protocol or schema changes add `!` and explain the decision in the body: `feat(store)!: ...`.

**Types:** `feat`, `fix`, `refactor`, `perf`, `test`, `docs`, `chore`, `ci`, `style`.
**Scopes:** `auth`, `net`, `chat`, `media`, `call`, `ui`, `store`, `crypto`, `session`, `ci`, `docs`, `harness`.

```
feat(chat): add quote-reply rendering
fix(session): clear expired zpw_sek before QR re-login
refactor(store): move schema bootstrap into versioned migrations
```

## Pull requests

### Pick the tier

| Tier | When | Requirements |
| :--- | :--- | :--- |
| **1 — Patch** | Typo, comment, docs, dependency bump with no API change | One-sentence summary, checks green |
| **2 — Standard** | Feature, bug fix, refactor, test | Conventional commits, PR body below, tests pass, no new clippy warnings |
| **3 — Critical** | Protocol, cipher or schema change; breaking change; release | Tier 2 plus atomic commits, design rationale, protocol evidence (`docs/zalo-specs/...`), manual verification steps, rollback note, human audit |

### PR body (Tier 2 and up)

```markdown
## What
-

## Why
-

## How
-

## Verification
- [ ] `cargo test --locked`
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] Manual: <device / flow / observed error_code>

## Risk & Rollback
-
```

### Before you ask for review

- [ ] `git status` and `git diff` reviewed; no `session.json`, `*.db`, `dump/`, `qr.png` or logs staged.
- [ ] Branch name matches the prefix table.
- [ ] Commits are atomic and conventional.
- [ ] `cargo fmt --all` and `cargo clippy` clean for touched code; `cargo test --locked` passes.
- [ ] Docs and this guide updated when behaviour or conventions change.
- [ ] PR body meets the tier requirement.

## Code comments

> Code tells you **how**. Comments tell you **why**. A comment that restates the code is a defect.

| Kind | Form | Required |
| :--- | :--- | :--- |
| Contract | `///` rustdoc, `//!` module docs | Every `pub` item and shared test helper |
| Rationale | `//` prose, `// SAFETY:` | Wherever the "why" is not obvious |
| Tracked debt | `// TODO(<issue-or-token>):` | Only when tracked |
| Banned | Echo comments, commented-out code, author/date tags, changelog notes, banner art | Never |

### Contracts

Add these rustdoc sections when they apply: `# Errors` (returns `Result`), `# Panics` (`panic!`, `unwrap`, `expect`, indexing), `# Safety` (`pub unsafe fn`), `# Invariants` (any `INV-N` the function enforces).

```rust
/// Decode a Base64 AES-CBC payload using a 16- or 32-byte key.
///
/// # Errors
/// Returns an error when the Base64 is malformed, the key length is not 16/32,
/// or the PKCS#7 padding is invalid.
///
/// # Invariants
/// - INV-1: callers must pass a key derived from `zpw_enk`, never a raw password.
pub fn decode_aes(key: &[u8], b64: &str) -> anyhow::Result<String> { ... }
```

Module docs state the module's role in 1–4 lines:

```rust
//! WebSocket transport for Zalo push/pull frames.
//!
//! Frame layout and command IDs are documented in `docs/zalo-specs/`.
```

### Rationale

```rust
// PC bundle ships a stale service map; the web/QR session returns *.chat.zalo.me hosts.
let host = service("chat").unwrap_or(MSG_CHAT);
```

- Plain prose, no `// WHY:` prefix. Place it next to the decision.
- Cite evidence for protocol behaviour: `// Observed 2026-10-21, error_code 0 (docs/zalo-specs/...)`.
- Every `unsafe` block has a `// SAFETY:` comment naming the invariant that makes it sound.

### TODOs

```rust
// TODO(#142): drop this fallback once password login supports 2FA.
// TODO(protocol): revisit when Zalo bumps zpw_ver past 694.
```

A bare `// TODO: fix this` is not allowed. Use an issue number, or a context token (`protocol`, `perf`, `ui`) when no issue exists. A TODO that survives a release without an issue must become one.

### Language

- Source comments and doc comments: **English**.
- User-facing strings (UI labels, toasts, CLI help): **Vietnamese**.
- Log messages: English, quoting Vietnamese server text only as values.
- Existing Vietnamese comments may stay, but convert the ones in any region you modify.

## Releases

- Tags follow `v<major>.<minor>.<patch>`; `Cargo.toml` is the only place the version lives.
- A release PR lists the tested platforms and artifacts.
- Never tag from a dirty worktree or with failing checks.
