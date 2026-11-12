// Sets each workspace package's version in package-lock.json to the one in its package.json.
// Called by scripts/bump-version.ps1. Keeps the file's line endings.
const fs = require("fs");
const path = require("path");

const lockPath = "package-lock.json";
const raw = fs.readFileSync(lockPath, "utf8");
const eol = raw.includes("\r\n") ? "\r\n" : "\n";
const lock = JSON.parse(raw);
let changed = 0;
for (const [key, entry] of Object.entries(lock.packages || {})) {
  if (!key.startsWith("apps/") && !key.startsWith("packages/")) continue;
  const pkgPath = path.join(key, "package.json");
  if (!fs.existsSync(pkgPath)) continue;
  const version = JSON.parse(fs.readFileSync(pkgPath, "utf8")).version;
  if (version && entry.version !== version) {
    entry.version = version;
    changed++;
  }
}
fs.writeFileSync(lockPath, JSON.stringify(lock, null, 2).replace(/\n/g, eol) + eol);
console.log("package-lock.json: " + changed + " workspace version(s) updated");
