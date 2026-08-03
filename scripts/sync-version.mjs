import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export function syncVersion(rootDir) {
  const versionPath = path.join(rootDir, "VERSION");
  const version = fs.readFileSync(versionPath, "utf8").trim();
  if (!version) {
    throw new Error("VERSION file is empty");
  }

  const pkgPath = path.join(rootDir, "package.json");
  const pkg = JSON.parse(fs.readFileSync(pkgPath, "utf8"));
  pkg.version = version;
  fs.writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + "\n");

  const cargoPath = path.join(rootDir, "src-tauri", "Cargo.toml");
  let cargo = fs.readFileSync(cargoPath, "utf8");
  cargo = cargo.replace(/^version\s*=\s*"[^"]*"/m, `version = "${version}"`);
  fs.writeFileSync(cargoPath, cargo);

  const confPath = path.join(rootDir, "src-tauri", "tauri.conf.json");
  const conf = JSON.parse(fs.readFileSync(confPath, "utf8"));
  conf.version = version;
  fs.writeFileSync(confPath, JSON.stringify(conf, null, 2) + "\n");

  return version;
}

const isMain =
  process.argv[1] &&
  path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url));

if (isMain) {
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const v = syncVersion(root);
  console.log(`Synced version ${v}`);
}
