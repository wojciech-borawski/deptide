import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const version = process.argv[2];

if (!version || !/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(version)) {
  console.error("usage: node scripts/set-version.mjs <major.minor.patch>");
  process.exit(1);
}

function rewrite(relativePath, pattern, replacement) {
  const file = path.join(root, relativePath);
  const before = readFileSync(file, "utf8");
  if (!pattern.test(before)) throw new Error(`No version found in ${relativePath}`);
  writeFileSync(file, before.replace(pattern, replacement));
  console.log(`${relativePath}: ${version}`);
}

const jsonVersion = /"version": "[^"]+"/;
const jsonReplacement = `"version": "${version}"`;

rewrite("package.json", jsonVersion, jsonReplacement);
rewrite("apps/desktop-ui/package.json", jsonVersion, jsonReplacement);
rewrite("crates/deptide-desktop/tauri.conf.json", jsonVersion, jsonReplacement);
rewrite("Cargo.toml", /(\[workspace\.package\][\s\S]*?version = )"[^"]+"/, `$1"${version}"`);
