# ADR 0035: Compact host development across worktrees

Status: Accepted and implemented

Date: 2026-10-21

## Context

Parallel worktrees each keep Cargo build output. Full debug information, incremental state and repeated guest/release builds use substantial local disk space. Sharing one Cargo target directory would make parallel builds compete for the same output. Release and guest verification also have fixed build settings and identity limits under [ADR 0034](0034-lez-v04-migration.md).

## Decision

Provide `verify/dev.sh` for routine host checks, tests and clippy in the application, actor foundation and weighted-math workspaces. Use their existing Rust pins and lockfiles. Keep outputs separate under each worktree's ignored `.local/launchpad/dev/` directory. Keep line tables for backtraces and disable incremental compilation. Apply these settings to the child Cargo process instead of changing workspace or global profiles.

When sccache is available, use one local compiler-cache server across worktrees, with a separate socket and a 5 GiB disk limit. Do not share Cargo target directories. Use a command-line target directory instead of `CARGO_TARGET_DIR`; sccache includes Cargo environment values in Rust cache keys, so a worktree-specific environment value would prevent reuse. Cache reuse depends on matching compiler inputs. Linked binaries, procedural macros and some other invocations remain outside sccache's Rust support.

Keep existing guest, release and reproducible verification commands unchanged. The development command cannot select their workspaces, target directories or profiles. Keep full verification in CI or a dedicated worktree. The [workspace guide](../architecture/workspace-guide.md#daily-development) defines the commands, cache controls and scoped cleanup.

## Consequences

Each worktree still stores its own compiled output. Smaller debug information reduces that output, while the shared cache can avoid recompiling matching dependencies. Builds without incremental state can take longer after source changes. Backtraces retain file and line information; full variable inspection requires a separate debugging build. sccache is optional, and the command reports when it is unavailable. Host development checks do not replace guest execution or release evidence.

## Sources

- [Cargo debug information and incremental profiles](https://doc.rust-lang.org/cargo/reference/profiles.html)
- [Cargo shared compiler cache](https://doc.rust-lang.org/cargo/reference/build-cache.html#shared-cache)
- [sccache Rust support](https://github.com/mozilla/sccache/blob/v0.18.0/docs/Rust.md)
- [sccache local cache and server limits](https://github.com/mozilla/sccache/blob/v0.18.0/docs/Local.md)
