# Weighted buys with a temporary clock read

Status: Historical mock implementation. Replaced by the [upstream clock integration](lez-clock-integration.md). The design and evidence below describe the earlier implementation.

## Goal

Remove buyer-supplied pricing windows from weighted buys. Read one timestamp through a temporary clock interface. Use that timestamp to select scheduled weights. Use a fake clock for local tests until LEZ supplies a supported timestamp reply.

The project assumes that a future LEZ version will support clock reads. Its exact interface is not known. This plan does not claim that the current LEZ clock supports timestamp replies. Keep the current LEZ dependency pin.

Defer the controls, observations, settlement and recovery discussion. Those decisions are outside this change.

## Intended behavior

- Exact-input buys take an input amount and a minimum output. Exact-output buys take an output amount and a maximum input. Neither buy takes a pricing timestamp or time window.
- Each buy requests time once. Store the reply only in that buy's pending operation. Do not reuse it for a later buy.
- The temporary reply contains Unix milliseconds. Convert it to Unix seconds with integer division. Use that second to interpolate collateral weight. Calculate token weight as its exact complement. Keep the existing normalized weights, rounding and math limits.
- Model the saved time of the previous block. It is not the current inclusion time. Reject pricing time before the sale start or at or after its end.
- Attach the sale's half-open start/end interval to the transaction response. These real LEZ inclusion bounds must reject a buy at or after the end even if the fake clock is stale.
- Keep buyer price limits, current pricing reserves, creator-borne fees, vault coverage, namespace freeze and atomic rollback.
- Keep fixed-price and bonding-curve behavior unchanged. Weighted setup keeps its current clock assertions and inclusion bounds.

## 1. Add the temporary clock boundary

Add a small module for the clock request, reply, expected actor and reply validation. Use a read-and-reply flow, not a host wall-clock call. Keep the provisional wire format separate from weighted math and pricing policy.

Select the expected clock actor through trusted build or deployment configuration. A buyer must not select or replace it. Do not expose a public timestamp setter on the sale program.

Add a fake actor for local guest tests. Its timestamp comes from fixture state. The fixture controls clock advances between transactions. Keep it at a distinct test address; do not replace the canonical LEZ clock used by setup and other mechanisms. Check both actor addresses on every reply. Reject unsolicited, malformed, repeated and out-of-order replies.

Mark the module and fake actor with `TEMPORARY` comments. State why they exist and that the replacement is a verified LEZ timestamp-read API. The normal build must not silently enable a fake time source. Provide an explicit local-test build configuration; without a supported clock provider, weighted clock reads must fail clearly.

## 2. Refactor the buy flow

Update `crates/lbp-core/src/lib.rs` to remove `valid_from` and `valid_until` from both weighted buy messages. No compatibility decoder is required.

Extend the pending trade flow in `crates/lbp-core/src/trading.rs` with an explicit clock-read phase. Preserve buyer identity, authorization and trade limits across the reply. Accept the reply only during that phase. Continue the existing namespace and token checks within the same transaction. A failure must restore the state from before the buy, including pending state.

Pass the validated time to the weighted pricing function. Replace the least-favourable-window calculation with one interpolation at that time. Remove weighted window parsing and buy-specific clock assertions. Retain inclusion bounds derived from the immutable sale schedule.

## 3. Update interfaces and callers

Regenerate `idl/launchpad.json` from its maintained source. Update wire checks, buy builders and all weighted buy fixtures. Search the full repository for the old fields, including generation scripts and documentation. Keep future transaction-expiry policy separate from the removed pricing inputs.

## 4. Verify locally

Host tests must cover both buy forms, rising/falling/constant weight schedules, fractional weights, millisecond conversion, exact schedule boundaries, slippage and math limits. Test repeated buys at the same timestamp with changed reserves. Test later timestamps with changed weights.

Protocol tests must reject forged clock senders, missing or malformed replies, duplicate replies, wrong phases and attempts to supply time through the public buy call. A failed read must not leave a pending trade or move funds.

Guest tests must execute the real sale and token guests with the fake clock guest. Cover successful buys, fee collection, freeze, and rollback after a final transfer failure. Keep inclusion time separate from fixture clock time. With clock time held before the sale end, prove that inclusion at the end still fails. Check complete state and signer nonce on rejection.

Run the host development gate, IDL and wire checks, formatting and relevant guest execution tests. Record exact commands and results. Report fake-clock execution as local integration evidence only; it does not prove compatibility with a future LEZ clock or private routes.

## 5. Update design and evidence records

Amend ADR 0026 with an explicit supersession of buyer-selected pricing windows when the refactor is implemented. Preserve the reason for the earlier design. State the temporary clock dependency and the distinction between saved pricing time and inclusion time.

Update CONTEXT.md, the weighted-sale overview and the workspace evidence guide to match the new behavior. Do not describe future LEZ support as implemented. Do not decide checkpoint behavior as part of this change.

## Completion and later replacement

The local refactor is complete when no weighted buy accepts a pricing timestamp or window, the sale obtains pricing time only through the temporary read interface, and the required host and guest checks pass.

When LEZ provides timestamp replies, replace the temporary provider and wire adapter. Verify its sender, units, update order, freshness and transaction semantics. Rerun the guest checks with the real clock, then remove the local-test build path where it is no longer needed. The weighted pricing function should not need a new policy change solely because the reply format changes.

## Local validation

The host development gate passed with unchanged lockfiles. All 13 existing sale guest tests passed with the fake clock. Two focused clock guest tests then passed, including the new malformed-state and frozen-sale checks. All 24 weighted host tests and all 30 client wire tests passed. The IDL generation check, formatting and shell syntax checks passed. Normal-build clock availability was checked separately with the mock variable unset. These are local development receipts, not real proofs or live clock evidence.

Use `bash verify/check-weighted-clock.sh` to reproduce the local host and guest checks. The host development script also runs the weighted host tests with the explicit fake provider.
