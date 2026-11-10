import { spawnSync } from "node:child_process"
import { createECDH, createHash, randomBytes } from "node:crypto"
import { mkdirSync, writeFileSync } from "node:fs"
import { fileURLToPath } from "node:url"
import { performance } from "node:perf_hooks"
import { CAPACITY, DEPTH, createAllowlist, verifyMembership } from "./merkle.ts"

const directory = fileURLToPath(new URL("../", import.meta.url))
const publicAccountPrefix = Buffer.from("/LEE/v0.3/AccountId/Public/\0\0\0\0\0")

type Measurement = {
  members: number
  depth: number
  accountPreparationSeconds: number
  listConstructionSeconds: number
  proofPreparationSeconds: number
  nodeVerificationSeconds: number
  childWallSeconds: number
  wallSeconds: number
  peakRssBytes: number
  witnessBytes: number
  policyBytes: number
}
type Attempt = {
  fixture: { members: number; policyHex: string; accountHex: string; proofHex: string }
  metrics: Measurement
}

function measure(members: number): Attempt {
  const started = performance.now()
  const accounts = Array.from({ length: members }, (_, i) => {
    const key = i === 0 ? Buffer.alloc(32, 1) : Buffer.alloc(32)
    if (i !== 0) key.writeUInt32BE(i + 1, 28)
    const curve = createECDH("secp256k1")
    curve.setPrivateKey(key)
    return createHash("sha256").update(publicAccountPrefix).update(curve.getPublicKey(undefined, "compressed").subarray(1)).digest()
  })
  const accountPreparationSeconds = (performance.now() - started) / 1000
  const context = Buffer.concat([Buffer.from([1]), randomBytes(32), randomBytes(32)])
  const listStart = performance.now()
  const list = createAllowlist(accounts, context)
  const listConstructionSeconds = (performance.now() - listStart) / 1000
  const proofStart = performance.now()
  const proof = list.proofFor(accounts[0])
  const proofPreparationSeconds = (performance.now() - proofStart) / 1000
  const verifyStart = performance.now()
  if (!verifyMembership(list.policy, accounts[0], proof)) throw new Error("Membership verification failed")
  const nodeVerificationSeconds = (performance.now() - verifyStart) / 1000
  return {
    fixture: { members, policyHex: list.policy.toString("hex"), accountHex: accounts[0].toString("hex"), proofHex: proof.toString("hex") },
    metrics: {
      members, depth: DEPTH, accountPreparationSeconds, listConstructionSeconds, proofPreparationSeconds,
      nodeVerificationSeconds, childWallSeconds: (performance.now() - started) / 1000, wallSeconds: 0,
      peakRssBytes: process.resourceUsage().maxRSS * 1024, witnessBytes: proof.length, policyBytes: list.policy.length
    }
  }
}

export function runAttempt(members: number): Attempt {
  if (!Number.isSafeInteger(members) || members < 1 || members > CAPACITY) throw new Error("Members must be between 1 and 131072")
  const start = performance.now()
  const child = spawnSync(process.execPath, ["--import", "tsx", fileURLToPath(import.meta.url), "--worker", String(members)], {
    cwd: directory, encoding: "utf8", timeout: 120_000, killSignal: "SIGKILL", maxBuffer: 1024 * 1024
  })
  if (child.error || child.status !== 0) throw new Error(`Membership child failed: ${child.error?.message ?? child.stderr.trim() ?? child.signal}`)
  const attempt: Attempt = JSON.parse(child.stdout)
  attempt.metrics.wallSeconds = (performance.now() - start) / 1000
  return attempt
}

if (process.argv[1] && fileURLToPath(import.meta.url) === fileURLToPath(new URL(`file://${process.argv[1]}`))) {
  if (process.argv[2] === "--worker") {
    const members = Number(process.argv[3])
    if (!Number.isSafeInteger(members) || members < 1 || members > CAPACITY) throw new Error("Invalid member count")
    process.stdout.write(JSON.stringify(measure(members)))
  } else {
    const args = process.argv.slice(2)
    const members = args.includes("--members") ? Number(args[args.indexOf("--members") + 1]) : 3
    const attempt = runAttempt(members)
    mkdirSync(`${directory}results`, { recursive: true })
    writeFileSync(`${directory}results/merkle-${members}-node.json`, `${JSON.stringify(attempt.metrics, null, 2)}\n`)
    if (args.includes("--fixtures")) {
      writeFileSync(`${directory}fixtures/merkle-${members}.json`, `${JSON.stringify(attempt.fixture, null, 2)}\n`)
    }
    console.log(JSON.stringify(attempt.metrics, null, 2))
  }
}
