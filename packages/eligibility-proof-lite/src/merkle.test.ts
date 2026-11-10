import assert from "node:assert/strict"
import test from "node:test"
import { readFileSync } from "node:fs"
import { createAllowlist, verifyMembership } from "./merkle.ts"

const vector = JSON.parse(readFileSync(new URL("../fixtures/merkle-vector.json", import.meta.url), "utf8"))
const account = Buffer.from(vector.accountHex, "hex")
const context = Buffer.from(vector.contextHex, "hex")
const zeroRandomness = (size: number) => Buffer.alloc(size)

test("an approved public trader gets the fixed SHA-256 membership vector", () => {
  const list = createAllowlist([account], context, { randomBytes: zeroRandomness })
  assert.equal(list.policy.toString("hex"), vector.policyHex)
  assert.equal(list.proofFor(account).toString("hex"), vector.proofHex)
  assert.deepEqual(list.proofFor(account), list.proofFor(account))
})

test("membership is reusable but cannot be copied to a different trader", () => {
  const policy = Buffer.from(vector.policyHex, "hex")
  const proof = Buffer.from(vector.proofHex, "hex")
  assert.equal(verifyMembership(policy, account, proof), true)
  assert.equal(verifyMembership(policy, account, proof), true)
  assert.equal(verifyMembership(policy, Buffer.alloc(32, 9), proof), false)
})

test("malformed and duplicate enrollments are rejected before building a list", () => {
  for (const addresses of [[], [account, account], [Buffer.alloc(31)], Array(131073).fill(account)]) {
    assert.throws(() => createAllowlist(addresses, context))
  }
  assert.throws(() => createAllowlist([account], context.subarray(1)))
  assert.throws(() => createAllowlist([account], Buffer.alloc(65, 2)))
  assert.throws(() => createAllowlist([account], context, { randomBytes: () => Buffer.alloc(1) }))
})

test("altered policy, salt, path, index and non-canonical encodings cannot prove membership", () => {
  const policy = Buffer.from(vector.policyHex, "hex")
  const proof = Buffer.from(vector.proofHex, "hex")
  for (const offset of [0, 1, 33, 65]) {
    const changed = Buffer.from(policy)
    changed[offset] ^= 1
    assert.equal(verifyMembership(changed, account, proof), false)
  }
  for (const offset of [0, 3, 4, 35, 36, 68, 579]) {
    const changed = Buffer.from(proof)
    changed[offset] ^= 1
    assert.equal(verifyMembership(policy, account, changed), false)
  }
  const swapped = Buffer.from(proof)
  proof.subarray(36, 68).copy(swapped, 68)
  proof.subarray(68, 100).copy(swapped, 36)
  for (const changed of [swapped, proof.subarray(1), Buffer.concat([proof, Buffer.alloc(1)]), Buffer.alloc(10000)]) {
    assert.equal(verifyMembership(policy, account, changed), false)
  }
  assert.equal(verifyMembership(policy.subarray(1), account, proof), false)
  assert.equal(verifyMembership(Buffer.concat([policy, Buffer.alloc(1)]), account, proof), false)
  assert.equal(verifyMembership(policy, account.subarray(1), proof), false)
})

test("a shuffled address batch gives each member its own path and excludes outsiders", () => {
  const accounts = [account, Buffer.alloc(32, 5), Buffer.alloc(32, 6)]
  const list = createAllowlist(accounts, context)
  const positions = new Set<number>()
  for (const member of accounts) {
    const proof = list.proofFor(member)
    positions.add(proof.readUInt32LE())
    assert.equal(verifyMembership(list.policy, member, proof), true)
  }
  assert.equal(positions.size, 3)
  assert.throws(() => list.proofFor(Buffer.alloc(32, 9)))
})
