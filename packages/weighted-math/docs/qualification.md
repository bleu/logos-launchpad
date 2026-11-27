# Weighted-math qualification

These checks apply to the unaudited kernel at `SCALE = 52` and `LN_SCALE = 62`. They do not qualify other scales or establish production weighted-sale support.

## Accuracy

The selected target is at most one basis point (`1/10000`) of extra arithmetic loss against the independently rounded ideal trade. Fees, slippage and normal integer rounding are separate. Reject trades that cannot meet the target. The [error analysis](error-analysis.md) defines the bounds; their hand derivation still needs mathematical review.

## Checks

The independent [oracle fixtures](../crates/harness/oracle/README.md) cover both quote directions, mixed decimals, large reserves, small buys and weight boundaries. The tests check rounding direction, error bounds and rejection cases. The [overflow checks](overflow-proof.md#verification) also run with release overflow checks disabled. The [guest checks](../zkvm/README.md) compare host and guest math results. Matching results do not establish numerical accuracy on their own.

## Real proof

The rebuilt guest image and the checked quote interface lack real-proof coverage. A receipt verifies only its original image. Full-operation proof costs remain unmeasured.

## Checked quotes

The checked quote interface uses the interval accuracy guards in the [error analysis](error-analysis.md#checked-quote-interface). These guards can certify some coarse-unit trades that the generic guards reject. They do not change the raw kernel results or the accuracy target. Host tests compare accepted quotes with the raw kernel and the independent oracle.

Sale guest tests cover checked quotes and rollback. See the [workspace guide](../../../docs/architecture/workspace-guide.md#execution-and-evidence-limits) for integration limits. Full-fixture host/guest parity and guest cost for the checked interface remain unverified.
