import assert from "node:assert/strict"
import test from "node:test"
import { readFileSync } from "node:fs"
import { groth16 } from "snarkjs"
import { verifyProof } from "@semaphore-protocol/proof"
import { runAttempt, verifyFor } from "./benchmark.ts"

const fixture = JSON.parse(readFileSync(new URL("../fixtures/proof.json", import.meta.url), "utf8"))
const key = JSON.parse(readFileSync(new URL("../fixtures/verification-key.json", import.meta.url), "utf8"))

test("a tiny sale allowlist produces a real depth 17 proof accepted by both official verifiers", async () => {
  const { fixture, verificationKey, metrics } = runAttempt(3)
  assert.equal(fixture.semaphoreProof.merkleTreeDepth, 17)
  assert.equal(await verifyProof(fixture.semaphoreProof), true)
  assert.equal(await groth16.verify(verificationKey, fixture.publicSignals, fixture.proof), true)
  assert.equal(await verifyFor(fixture, verificationKey, fixture.policy), true)
  assert.ok(metrics.proofSeconds > 0)
  assert.ok(metrics.peakRssBytes > 0)
})

test("a proof cannot authorize a different allowlist root", async () => {
  assert.equal(await verifyFor(fixture, key, { ...fixture.policy, root: (BigInt(fixture.policy.root) + 1n).toString() }), false)
})

test("a proof cannot authorize a different sale scope", async () => {
  assert.equal(await verifyFor(fixture, key, { ...fixture.policy, saleScope: "8" }), false)
})

test("a proof cannot authorize a different purchase message", async () => {
  assert.equal(await verifyFor(fixture, key, { ...fixture.policy, purchaseMessage: "43" }), false)
})
