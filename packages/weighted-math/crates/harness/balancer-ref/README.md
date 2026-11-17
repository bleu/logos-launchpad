# Balancer reference capture

CI reads the committed `../fixtures/balancer_pow.json` offline. It does not run Foundry.

Balancer's `LogExpMath` is the secondary comparator under [ADR 0014](../../../../../docs/adr/0014-weighted-math-evidence.md). The grader checks its captured outputs against the mpmath reference using Balancer's documented accuracy: 1e-14 relative on its 1e18 grid, plus 2 wei of rounding. This checks the grader against an independent implementation; it does not qualify the launchpad kernel by itself.

## Pinned source

- Repo: `balancer/balancer-v3-monorepo`
- Commit: `7861ea2785b96dd10681ff1b8dfe56b36cc202b6`
- File: `pkg/solidity-utils/contracts/math/LogExpMath.sol`

The source is fetched at capture time into `src-external/` (gitignored), not vendored. The v3 file is MIT-licensed; the earlier GPL-3.0 concern applied to Balancer v2. Only the captured numbers (data) and our script live in this repo.

## Regenerate

Run from this directory:

```sh
./capture.sh   # needs foundry + network; writes ../fixtures/balancer_pow.json
```

`../oracle/gen.py` emits the committed `inputs_flat.json` from the shared power fixture. The capture script reads it and runs `LogExpMath.pow` for each case. It records `null` for the 11 cases outside Balancer's exponential domain.
