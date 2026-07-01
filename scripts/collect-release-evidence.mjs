#!/usr/bin/env node
/**
 * Collects QUALITY_GATE_SPEC / RELEASE_SPEC release evidence bundle.
 *
 * Default mode writes metadata (commands + artifact paths) without running them.
 * `--run` executes each verification command from component.spec.json and records
 * the outcome (exit code, duration, stdout tail) so the bundle carries real
 * completion evidence per QUALITY_GATE_SPEC §5-§6 ("name evidence, not confidence";
 * "Completion evidence records commands and outcomes").
 */

import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const outputDir = path.join(repoRoot, "target", "release-evidence");
const outputPath = path.join(outputDir, "release-evidence.json");

const runMode = process.argv.includes("--run");

function gitRev() {
  const result = spawnSync("git", ["rev-parse", "HEAD"], {
    cwd: repoRoot,
    encoding: "utf8",
  });
  if (result.status !== 0) {
    return "unknown";
  }
  return result.stdout.trim();
}

const componentSpec = JSON.parse(
  readFileSync(path.join(repoRoot, "specs", "component.spec.json"), "utf8"),
);

/**
 * Execute a single verification command and capture its outcome.
 * Commands like `cd apps/x && npm run y` are executed in a shell so the `cd` chain works.
 */
function runVerificationCommand(command) {
  const start = Date.now();
  const result = spawnSync(command, {
    cwd: repoRoot,
    encoding: "utf8",
    shell: true,
    maxBuffer: 4 * 1024 * 1024,
  });
  const durationMs = Date.now() - start;
  const stdout = (result.stdout ?? "").trimEnd();
  const stderr = (result.stderr ?? "").trimEnd();
  const tail = stdout.length > 800 ? `...${stdout.slice(-800)}` : stdout;
  return {
    command,
    exitCode: result.status ?? -1,
    durationMs,
    stdoutTail: tail || null,
    stderrTail: stderr ? (stderr.length > 400 ? `...${stderr.slice(-400)}` : stderr) : null,
  };
}

const verification = {
  entrypoints: ["scripts/verify.ps1", "scripts/verify.sh"],
  commands: componentSpec.verification.commands,
};

if (runMode) {
  const results = [];
  for (const command of componentSpec.verification.commands) {
    process.stdout.write(`release-evidence: running "${command}"...\n`);
    const outcome = runVerificationCommand(command);
    results.push(outcome);
    const status = outcome.exitCode === 0 ? "OK" : `FAIL(${outcome.exitCode})`;
    process.stdout.write(`release-evidence: ${status} (${outcome.durationMs}ms)\n`);
  }
  verification.results = results;
  verification.allPassed = results.every((item) => item.exitCode === 0);
}

const bundle = {
  schemaVersion: 1,
  kind: "sdkwork.web-framework.release-evidence",
  collectedAt: new Date().toISOString(),
  gitCommit: gitRev(),
  component: componentSpec.component,
  metadata: componentSpec.metadata,
  verification,
  releaseArtifacts: {
    changelog: "CHANGELOG.md",
    rolloutDoc: "docs/architecture/tech/TECH-24-production-rollout-and-adoption.md",
    adoptionTemplate: "specs/production-adoption.evidence.template.json",
    frameworkPathfinderAdoptions: "specs/framework-adoption.evidence.json",
    adminServerBinary: "target/release/sdkwork-web-admin-server",
    workflowManifest: "sdkwork.workflow.json",
  },
  benchmark: {
    command:
      "cargo test --release -p sdkwork-web-architecture-tests --test pipeline_benchmark",
    scripts: ["scripts/benchmark-pipeline.ps1", "scripts/benchmark-pipeline.sh"],
  },
  deploymentProfile: "cloud",
  runtimeTarget: "server",
};

mkdirSync(outputDir, { recursive: true });
writeFileSync(outputPath, `${JSON.stringify(bundle, null, 2)}\n`, "utf8");

if (process.argv.includes("--print-path")) {
  console.log(outputPath);
} else {
  const suffix = runMode
    ? ` (${verification.results.length} commands, allPassed=${verification.allPassed})`
    : "";
  console.log(`release-evidence: wrote ${outputPath}${suffix}`);
}
