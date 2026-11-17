# ADR 0011: Weighted precision

Status: Accepted (reviewed PoC baseline; unaudited)

## Decision

Use public scale `2^52` and internal scale `2^62`. Keep `1 - power` at internal scale through the final widened multiply: subtract `exp_result` from `2^62` and apply the pool-safety pad `2^-53`. Do not add a conditional `expm1` swap branch.

## Reason and limits

Internal-scale subtraction preserves small complements that public-scale rounding would lose. The retained precision evidence supports scale 52 within its stated error and arithmetic bounds; higher public scales need new evidence. Any scale or cancellation-path change requires new error and overflow checks. See [error analysis](../../packages/weighted-math/docs/error-analysis.md) and [qualification](../../packages/weighted-math/docs/qualification.md). These checks are not an audit or a production-sale precision guarantee.

## Sources

- [Error analysis](../../packages/weighted-math/docs/error-analysis.md)
- [Oracle fixtures and regeneration](../../packages/weighted-math/crates/harness/oracle/README.md)
- [0003-fixed-point-scale.md](https://github.com/bleu/lez-weighted-math/blob/f99195f4bcc375940171e4ee5373e67592622800/docs/adr/0003-fixed-point-scale.md)
- [0005-expm1-boundary.md](https://github.com/bleu/lez-weighted-math/blob/f99195f4bcc375940171e4ee5373e67592622800/docs/adr/0005-expm1-boundary.md)
