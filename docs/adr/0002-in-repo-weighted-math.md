# ADR 0002: Maintain weighted math as an internal package

Status: Accepted

Date: 2026-09-29

## Context

The weighted kernel has numerical tests and guest-parity evidence, but it has not been audited. LBP integration will need input qualification and may require kernel changes. An external-only dependency adds a separate release step to this work.

## Decision

The imported weighted-math source is an independent workspace in `packages/weighted-math/`, with its own kernel package, harness, fixtures, proof material and guest-parity workspace. A future LBP can consume the kernel through a local Cargo path dependency. Keep its API free of LEZ accounts and launch policy so other applications can reuse it.

The import is clean and has no source history. Bleu owns its code. Keep the math's MIT license and required third-party notices. Archive the old math repository only after the checked import is merged.

## Consequences

Math and LBP changes can be reviewed together without merging their responsibilities. Independent package tests remain required. Copying the kernel neither audits it nor qualifies all production inputs. Package publication is a separate decision.

## Sources

- [Reviewed math source](https://github.com/bleu/lez-weighted-math/tree/f99195f4bcc375940171e4ee5373e67592622800)
