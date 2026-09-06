import { copyFileSync, mkdirSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const releaseDirectory = path.join(root, "target", "release");
const targetDirectory = path.join(root, "release");
const suffix = process.platform === "win32" ? ".exe" : "";

const binaries = [
  { source: `deptide${suffix}`, target: `Deptide${suffix}` },
  { source: `deptide-cli${suffix}`, target: `deptide-cli${suffix}` },
];

mkdirSync(targetDirectory, { recursive: true });

for (const binary of binaries) {
  const source = path.join(releaseDirectory, binary.source);
  const target = path.join(targetDirectory, binary.target);
  copyFileSync(source, target);
  const size = (statSync(target).size / (1024 * 1024)).toFixed(1);
  console.log(`Portable build ready: ${target} (${size} MB)`);
}
