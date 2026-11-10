import { createHash, randomBytes } from "node:crypto"

export const DEPTH = 17
export const CAPACITY = 2 ** DEPTH
export const CONTEXT_BYTES = 65
export const POLICY_BYTES = CONTEXT_BYTES + 32
export const PROOF_BYTES = 4 + 32 + DEPTH * 32

const leafTag = Buffer.from("LP/allowlist/leaf/v1\0")
const paddingTag = Buffer.from("LP/allowlist/pad/v1\0")
const nodeTag = Buffer.from("LP/allowlist/node/v1\0")
const hash = (...parts: Uint8Array[]) => {
  const digest = createHash("sha256")
  for (const part of parts) digest.update(part)
  return digest.digest()
}
const uint32 = (value: number) => {
  const bytes = Buffer.alloc(4)
  bytes.writeUInt32LE(value)
  return bytes
}

export function verifyMembership(policy: Uint8Array, account: Uint8Array, witness: Uint8Array): boolean {
  if (policy.length !== POLICY_BYTES || policy[0] !== 1 || account.length !== 32 || witness.length !== PROOF_BYTES) return false
  const proof = Buffer.from(witness)
  let index = proof.readUInt32LE(0)
  if (index >= CAPACITY) return false
  const context = policy.subarray(0, CONTEXT_BYTES)
  let digest = hash(leafTag, context, account, proof.subarray(4, 36))
  for (let level = 0; level < DEPTH; level++) {
    const sibling = proof.subarray(36 + level * 32, 68 + level * 32)
    digest = index & 1 ? hash(nodeTag, context, sibling, digest) : hash(nodeTag, context, digest, sibling)
    index >>>= 1
  }
  return digest.equals(policy.subarray(CONTEXT_BYTES))
}

export function createAllowlist(
  accounts: readonly Uint8Array[],
  scope: Uint8Array,
  options: { randomBytes?: (size: number) => Uint8Array } = {}
): { policy: Buffer; proofFor(account: Uint8Array): Buffer } {
  if (scope.length !== CONTEXT_BYTES || scope[0] !== 1) throw new Error("Unsupported policy context")
  if (accounts.length < 1 || accounts.length > CAPACITY) throw new Error("Approve between 1 and 131072 addresses")
  const seen = new Set<string>()
  for (const account of accounts) {
    if (account.length !== 32) throw new Error("Account IDs must be 32 bytes")
    const id = Buffer.from(account).toString("hex")
    if (seen.has(id)) throw new Error("Duplicate approved account")
    seen.add(id)
  }
  const context = Buffer.from(scope)
  const entropy = (size: number) => {
    const bytes = Buffer.from((options.randomBytes ?? randomBytes)(size))
    if (bytes.length !== size) throw new Error("Incorrect random byte count")
    return bytes
  }
  const slots = Array.from({ length: CAPACITY }, (_, i) => i)
  const members = new Map<string, { index: number; salt: Buffer }>()
  for (let i = 0; i < accounts.length; i++) {
    const range = CAPACITY - i
    const bound = 2 ** 32 - (2 ** 32 % range)
    let draw: number
    do { draw = Buffer.from(entropy(4)).readUInt32LE() } while (draw >= bound)
    const selected = i + draw % range
    ;[slots[i], slots[selected]] = [slots[selected], slots[i]]
    members.set(Buffer.from(accounts[i]).toString("hex"), { index: slots[i], salt: Buffer.from(entropy(32)) })
  }
  const leaves = Array.from({ length: CAPACITY }, (_, i) => hash(paddingTag, context, uint32(i)))
  for (const [account, { index, salt }] of members) {
    leaves[index] = hash(leafTag, context, Buffer.from(account, "hex"), salt)
  }
  const levels = [leaves]
  while (levels.at(-1)!.length > 1) {
    const previous = levels.at(-1)!
    levels.push(Array.from({ length: previous.length / 2 }, (_, i) => hash(nodeTag, context, previous[i * 2], previous[i * 2 + 1])))
  }
  return {
    policy: Buffer.concat([context, levels.at(-1)![0]]),
    proofFor(account) {
      const member = members.get(Buffer.from(account).toString("hex"))
      if (!member) throw new Error("Account is not approved")
      let index = member.index
      const siblings = levels.slice(0, DEPTH).map(level => {
        const sibling = level[index ^ 1]
        index >>>= 1
        return sibling
      })
      return Buffer.concat([uint32(member.index), member.salt, ...siblings])
    }
  }
}
