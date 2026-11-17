# ADR 0002: Maintain weighted math as an internal package

Status: Accepted

## Decision

Maintain weighted math in `packages/weighted-math/` as an independent workspace with its kernel, harness, fixtures, proof material and guest-parity workspace. Sale code can use a local Cargo path dependency. Keep LEZ accounts and launch policy outside the kernel.

## Reason and limits

Local maintenance permits related math and sale changes without a separate release. Keep the MIT license and required third-party notices. Independent tests and input qualification remain required. The kernel is unaudited; package publication is a separate decision.

## Sources

- [Reviewed math source](https://github.com/bleu/lez-weighted-math/tree/f99195f4bcc375940171e4ee5373e67592622800)
