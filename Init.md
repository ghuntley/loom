<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# Loom

> [!CAUTION]
> **Loom is a research project. If your name is not Geoffrey Huntley then do not use.**
>
> This software is experimental, unstable, and under active development. APIs will change without notice. Features may be incomplete or broken. There is no support, no documentation guarantees, and no warranty of any kind. Use at your own risk.

This file provides guidance to Claude Code (claude.ai/code) and other AI coding assistants when working with code in this repository.

## Overview

Loom is an AI-powered coding agent built in Rust. It provides a REPL interface for interacting with LLM-powered agents that can execute tools to perform file system operations, code analysis, and other development tasks.

### Key Components

| Component | Description |
|-----------|-------------|
| **Core Agent** | State machine for conversation flow and tool orchestration |
| **LLM Proxy** | Server-side proxy architecture - API keys never leave the server |
| **Tool System** | Registry and execution framework for agent capabilities |
| **Weaver** | Remote execution environments via Kubernetes pods |
| **Thread System** | Conversation persistence with FTS5 search |
| **Analytics** | PostHog-style product analytics with identity resolution |
| **Auth** | OAuth, magic links, ABAC authorization |
| **Feature Flags** | Runtime feature toggles, experiments, and kill switches |

### Server-Side LLM Proxy

```
┌─────────────┐      HTTP       ┌─────────────┐     Provider API    ┌─────────────┐
│  loom-cli   │ ───────────────▶│ loom-server │ ──────────────────▶ │  Anthropic  │
│             │ /proxy/{provider}│             │                     │   OpenAI    │
│ ProxyLlm-   │  /complete      │  LlmService │                     │    etc.     │
│ Client      │  /stream        │             │                     │             │
└─────────────┘ ◀─────────────  └─────────────┘ ◀────────────────── └─────────────┘
                  SSE stream                        SSE stream
```

API keys are stored server-side only. Clients communicate through the proxy.

## Specifications

**IMPORTANT:** Before implementing any feature, consult the specifications in `specs/README.md`.

- **Assume NOT implemented.** Many specs describe planned features that may not yet exist in the codebase.
- **Check the codebase first.** Before concluding something is or isn't implemented, search the actual code. Specs describe intent; code describes reality.
- **Use specs as guidance.** When implementing a feature, follow the design patterns, types, and architecture defined in the relevant spec.
- **Spec index:** `specs/README.md` lists all specifications organized by category (core, LLM, security, etc.).

## Commands

### Building with Nix (Preferred)
Use cargo2nix for reproducible builds with per-crate caching. Much faster on incremental changes.

- **Build CLI:** `nix build .#loom-cli-c2n`
- **Build server:** `nix build .#loom-server-c2n`
- **Build any crate:** `nix build .#<crate-name>-c2n` (e.g., `nix build .#loom-common-http-c2n`)
- **Build images:** `nix build .#weaver-image` or `nix build .#loom-server-image`
- **Update Cargo.nix:** `cargo2nix-update` (after modifying Cargo.lock)

### Building with Cargo (Development)
Use cargo for quick iteration during development. Slower than nix on clean builds.

- **Build:** `cargo build --workspace`
- **Test all:** `cargo test --workspace`
- **Test single:** `cargo test -p loom-<crate> <test_name>` (e.g., `cargo test -p loom-core test_agent`)
- **Lint:** `cargo clippy --workspace -- -D warnings`
- **Format:** `cargo fmt --all`
- **Check all:** `make check` (format + lint + build + test)

### Web
- **Web dev:** `cd web/loom-web && pnpm dev`
- **Web test:** `pnpm test`

### cargo2nix Workflow
When you modify `Cargo.toml` or `Cargo.lock`:
1. Run `cargo2nix-update` to regenerate `Cargo.nix`
2. Commit `Cargo.nix` along with your changes
3. The nix build will use the updated dependency graph

## Deployment
Deployments happen automatically via `git push` to the `trunk` branch. The production server runs NixOS with auto-update enabled. The update service runs every 10 seconds, checks for new commits, and rebuilds if needed.

- **Deploy:** `git push origin trunk`
- **Check status:** `sudo systemctl status nixos-auto-update.service`
- **View logs:** `sudo journalctl -u nixos-auto-update.service -f` (follow) or `-n 100` (last 100 lines)
- **Check deployed revision:** `cat /var/lib/nixos-auto-update/deployed-revision`
- **Force rebuild:** Delete the deployed revision file and restart: `sudo rm /var/lib/nixos-auto-update/deployed-revision && sudo systemctl start nixos-auto-update.service`
- **Service state:** `activating` = deploying, `active (exited)` = completed successfully
- **Repo location on server:** `/var/lib/depot`

### Verifying Deployment
0. IMPORTANT: You are running on the machine and can check status without ssh (just use sudo)
1. Check the deployed revision matches your commit: `cat /var/lib/nixos-auto-update/deployed-revision`
2. Check loom-server was restarted: `sudo systemctl status loom-server` (look at start time)
3. Check health endpoint: `curl -s https://loom.ghuntley.com/health | jq .`

## Database Migrations

**All database migrations go in `crates/loom-server/migrations/`** as numbered SQL files.

- **Convention:** `NNN_description.sql` (e.g., `020_scm_repos.sql`)
- **DO NOT** put inline SQL migrations in other crates (loom-server-scm, loom-thread, etc.)
- Migrations run automatically on server startup via `db/mod.rs`
- Check existing migrations for the next available number before creating new ones

### IMPORTANT: Force Rebuild After Adding Migrations

**cargo2nix doesn't track `include_str!` file changes.** When you add or modify migration files:

1. Run `cargo2nix-update` to regenerate `Cargo.nix` (this changes the hash and forces rebuild)
2. Commit `Cargo.nix` along with your migration changes
3. Without this step, the deployed binary will NOT include the new migration!

## Local Testing
Before deploying, test changes locally to verify behavior:

- **Run server on alternate port:** `LOOM_SERVER_PORT=9090 LOOM_SERVER_DB_PATH=/tmp/loom-test.db ./target/release/loom-server`
- **Dev mode (auto-auth):** Add `LOOM_SERVER_AUTH_DEV_MODE=1` for testing without real auth
- **Test against local:** `curl http://localhost:9090/health`
- **Run integration tests:** `cargo test -p loom-server <test_name>`

## Weaver Troubleshooting
Weavers are ephemeral K8s pods for running remote Loom REPL sessions.

### CLI Commands
- **Login first:** `loom --server-url https://loom.ghuntley.com login`
- **List weavers:** `loom --server-url https://loom.ghuntley.com weaver ps`
- **Create weaver:** `loom --server-url https://loom.ghuntley.com new --image <image>`
- **Attach:** `loom --server-url https://loom.ghuntley.com attach <weaver-id>`
- **Delete:** `loom --server-url https://loom.ghuntley.com weaver delete <weaver-id>`

### Kubernetes Debugging
Weavers run in the `loom-weavers` namespace:

- **List pods:** `sudo kubectl get pods -n loom-weavers`
- **Describe pod:** `sudo kubectl describe pod <pod-name> -n loom-weavers`
- **Pod logs:** `sudo kubectl logs <pod-name> -n loom-weavers`
- **Delete stuck pod:** `sudo kubectl delete pod <pod-name> -n loom-weavers`

### Server Logs
- **loom-server logs:** `journalctl -u loom-server -f` (follow) or `-n 100` (last 100 lines)

### Common Issues
- **ErrImagePull:** The container image doesn't exist or is private. Check `kubectl describe pod` for details.
- **Succeeded status immediately:** The container exited because it has no long-running entrypoint. Weaver images must run a persistent process (e.g., the loom REPL).
- **401 Unauthorized:** Run `loom --server-url <url> login` first to authenticate.

## Architecture
Rust workspace with 90+ crates under `crates/`. Key crates: `loom-core` (agent logic), `loom-server` (HTTP API), `loom-thread` (conversation state), `loom-llm-*` (LLM providers), `loom-tools` (agent tools), `loom-auth*` (authentication). Web frontend in `web/loom-web` (SvelteKit + Tailwind). SQLite database (`sqlx`). Dev environment via `devenv.nix`. Infra in `infra/` (Nix/K8s).

```
loom/
├── crates/
│   ├── loom-cli*/          # CLI and related tools
│   ├── loom-common-*/      # Shared utilities (config, http, i18n, secrets)
│   ├── loom-server*/       # HTTP API server and subsystems
│   ├── loom-tui-*/         # Terminal UI components
│   ├── loom-weaver-*/      # Remote execution (K8s pods)
│   └── loom-wgtunnel-*/    # WireGuard tunneling
├── web/
│   └── loom-web/           # Svelte 5 web frontend
├── specs/                  # Design specifications
└── infra/                  # Nix/K8s infrastructure
```

**Routes:** Use `PublicRouter` for unauthenticated routes, `AuthedRouter` for protected routes (see `typed_router.rs`). If unsure, ask. When adding/modifying routes, update authz tests in `tests/authz_*_tests.rs`.

## Svelte 5 (NOT Svelte 4)
**Always use Svelte 5 runes syntax. Never use Svelte 4 patterns.**

| Category | ✅ Svelte 5 | ❌ Svelte 4 (DO NOT USE) |
|----------|-------------|--------------------------|
| **State** | `let count = $state(0);` | `let count = 0;` |
| **Derived** | `const doubled = $derived(count * 2);` | `$: doubled = count * 2;` |
| **Effects** | `$effect(() => { ... });` | `$: { ... }` |
| **Props** | `let { foo, bar } = $props();` | `export let foo;` |
| **Events** | `onclick={handler}` | `on:click={handler}` |
| **Custom events** | Pass callback props: `onsave={fn}` | `createEventDispatcher` |
| **Slots** | `{@render children()}` | `<slot />` |

Stores (`writable`, `$store`) are supported but prefer runes for component state.

## Code Style
- **Formatting:** Hard tabs, 2-space width, 100 char line width (rustfmt.toml)
- **Errors:** Use `thiserror` for error enums, `anyhow` for propagation. Define `Result<T>` type aliases.
- **Async:** Tokio runtime. Use `async-trait` for async trait methods.
- **Imports:** Group std, external crates, then internal `loom-*` crates.
- **Naming:** snake_case for functions/variables, PascalCase for types, SCREAMING_CASE for constants.
- **No comments** unless code is complex and requires context for future developers. Copyright header required.
- **HTTP clients:** Never build `reqwest::Client` directly. Use `loom-http::{new_client, builder}` for consistent User-Agent and retry logic.
- **Testing:** Prefer property-based tests (`proptest`) over unit tests when appropriate; use unit tests for simple cases.
- **Logging:** Use structured logging (`tracing`). Never log secrets directly.
- **Instrumentation:** Use `#[instrument(skip(self, secrets, large_args), fields(id = %id))]`. Always skip secrets.
- **Secrets:** Use `loom-secret::{Secret, SecretString}` for API keys, tokens, passwords. Access via `.expose()`. Auto-redacts in Debug/Display/Serialize/tracing.

## Internationalization (i18n)

### Crate
Use `loom-i18n` for all translatable strings. Uses GNU gettext with `.po` files compiled to `.mo` at build time.

### String Naming Convention
All translatable strings use hierarchical dot-notation: `{prefix}.{domain}.{component}.{element}`

| Prefix | Usage | Example |
|--------|-------|---------|
| `server.` | Backend strings (emails, API responses) | `server.email.magic_link.subject` |
| `client.` | CLI strings (loom-cli output) | `client.error.connection_failed` |

### Domains
- `email` - Email subjects and bodies
- `api` - API response messages
- `auth` - Authentication messages
- `org` - Organization-related messages

### Usage
```rust
use loom_i18n::{t, t_fmt, is_rtl, resolve_locale};

// Simple translation
let subject = t("es", "server.email.magic_link.subject");

// Translation with variables (use {name} syntax)
let body = t_fmt("es", "server.email.invitation.subject", &[
    ("org_name", "Acme Corp"),
]);

// Resolve locale: user preference → server default → "en"
let locale = resolve_locale(user.locale.as_deref(), &config.default_locale);

// RTL support for HTML emails
if is_rtl(locale) {
    // Use dir="rtl" in HTML
}
```

### Adding Translations
1. Add msgid/msgstr to `crates/loom-i18n/locales/{locale}/messages.po`
2. Run `cargo build -p loom-i18n` to compile `.po` → `.mo`
3. Supported locales: `en` (English), `es` (Spanish), `ar` (Arabic/RTL)

### RTL Languages
Arabic (`ar`) and other RTL locales require `dir="rtl"` on HTML elements. Use `loom_i18n::is_rtl()` to check.

## Design Patterns

- When multiple code paths do similar things with slight variations, create a shared service with a request struct that captures the variations, rather than having each caller implement its own logic.

## License

Proprietary. Copyright (c) 2025 Geoffrey Huntley. All rights reserved.
