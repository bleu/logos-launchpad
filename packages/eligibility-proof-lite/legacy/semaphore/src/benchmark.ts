import { spawnSync } from "node:child_process"
import { mkdirSync, writeFileSync } from "node:fs"
import { fileURLToPath } from "node:url"
import { performance } from "node:perf_hooks"
import { Group } from "@semaphore-protocol/group"
import { Identity } from "@semaphore-protocol/identity"
import { generateProof, unpackGroth16Proof, verifyProof, type SemaphoreProof } from "@semaphore-protocol/proof"
import { keccak256, toBeHex } from "ethers"
import { groth16, zKey } from "snarkjs"

const packageDirectory = fileURLToPath(new URL("../", import.meta.url))
const artifact = (extension: string) => `${packageDirectory}artifacts/semaphore-17.${extension}`
const hash = (value: string) => (BigInt(keccak256(toBeHex(BigInt(value), 32))) >> 8n).toString()

type Policy = { root: string; saleScope: string; purchaseMessage: string }
type Fixture = {
  proof: ReturnType<typeof unpackGroth16Proof>
  publicSignals: string[]
  semaphoreProof: SemaphoreProof
  policy: Policy
}
type Attempt = {
  fixture: Fixture
  verificationKey: object
  metrics: { members: number; depth: number; groupSeconds: number; proofSeconds: number; wallSeconds: number; peakRssBytes: number }
}

export async function verifyFor(fixture: Fixture, key: object, policy: Policy): Promise<boolean> {
  return groth16.verify(key, [policy.root, fixture.publicSignals[1], hash(policy.purchaseMessage), hash(policy.saleScope)], fixture.proof)
}

export function runAttempt(members: number): Attempt {
  if (!Number.isSafeInteger(members) || members < 1 || members > 2 ** 17) {
    throw new Error("Members must be an integer between 1 and 131072")
  }
  const start = performance.now()
  const child = spawnSync(process.execPath, ["--import", "tsx", fileURLToPath(import.meta.url), "--worker", String(members)], {
    cwd: packageDirectory,
    encoding: "utf8",
    timeout: 120_000,
    killSignal: "SIGKILL",
    maxBuffer: 1024 * 1024
  })
  if (child.error || child.status !== 0) {
    throw new Error(`Proof child failed: ${child.error?.message ?? child.stderr.trim() ?? child.signal}`)
  }
  const attempt: Attempt = JSON.parse(child.stdout)
  attempt.metrics.wallSeconds = (performance.now() - start) / 1000
  return attempt
}

async function prove(members: number): Promise<Attempt> {
  const start = performance.now()
  const identity = new Identity()
  const group = new Group([identity.commitment, ...Array.from({ length: members - 1 }, (_, i) => BigInt(i + 1))])
  const groupSeconds = (performance.now() - start) / 1000
  const proofStart = performance.now()
  const semaphoreProof = await generateProof(identity, group, 42n, 7n, 17, {
    wasm: artifact("wasm"),
    zkey: artifact("zkey")
  })
  const proofSeconds = (performance.now() - proofStart) / 1000
  const verificationKey = await zKey.exportVerificationKey(artifact("zkey"))
  const fixture: Fixture = {
    proof: unpackGroth16Proof(semaphoreProof.points),
    publicSignals: [semaphoreProof.merkleTreeRoot, semaphoreProof.nullifier, hash(semaphoreProof.message), hash(semaphoreProof.scope)],
    semaphoreProof,
    policy: { root: semaphoreProof.merkleTreeRoot, saleScope: semaphoreProof.scope, purchaseMessage: semaphoreProof.message }
  }
  if (!(await verifyProof(semaphoreProof)) || !(await groth16.verify(verificationKey, fixture.publicSignals, fixture.proof))) {
    throw new Error("Official SDK or independent snarkjs verification failed")
  }
  return {
    fixture,
    verificationKey,
    metrics: { members, depth: 17, groupSeconds, proofSeconds, wallSeconds: (performance.now() - start) / 1000, peakRssBytes: process.resourceUsage().maxRSS * 1024 }
  }
}

if (process.argv[1] && fileURLToPath(import.meta.url) === fileURLToPath(new URL(`file://${process.argv[1]}`))) {
  if (process.argv[2] === "--worker") {
    try {
      const result = await prove(Number(process.argv[3]))
      process.stdout.write(JSON.stringify(result), () => process.exit(0))
    } catch (error) {
      process.stderr.write(`${error instanceof Error ? error.message : "Proof failed"}\n`, () => process.exit(1))
    }
  } else {
    const args = process.argv.slice(2)
    const members = args.includes("--members") ? Number(args[args.indexOf("--members") + 1]) : 3
    const attempts = args.includes("--attempts") ? Number(args[args.indexOf("--attempts") + 1]) : 1
    if (!Number.isSafeInteger(attempts) || attempts < 1) throw new Error("Attempts must be a positive integer")
    const results = Array.from({ length: attempts }, () => runAttempt(members))
    mkdirSync(`${packageDirectory}results`, { recursive: true })
    writeFileSync(`${packageDirectory}results/node.json`, `${JSON.stringify(results.map(({ metrics }) => metrics), null, 2)}\n`)
    if (args.includes("--fixtures")) {
      mkdirSync(`${packageDirectory}fixtures`, { recursive: true })
      writeFileSync(`${packageDirectory}fixtures/proof.json`, `${JSON.stringify(results[0].fixture, null, 2)}\n`)
      writeFileSync(`${packageDirectory}fixtures/verification-key.json`, `${JSON.stringify(results[0].verificationKey, null, 2)}\n`)
    }
    console.log(JSON.stringify(results.map(({ metrics }) => metrics), null, 2))
    process.exit(0)
  }
}
