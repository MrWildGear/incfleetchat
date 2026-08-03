import { describe, expect, it, beforeEach, afterEach } from "vitest";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { syncVersion } from "./sync-version.mjs";

function writeFixture(root: string) {
  fs.writeFileSync(path.join(root, "VERSION"), "1.2.3-preview.1\n");
  fs.writeFileSync(
    path.join(root, "package.json"),
    JSON.stringify({ name: "incfleetchat", version: "0.0.0" }, null, 2) + "\n",
  );
  fs.mkdirSync(path.join(root, "src-tauri"), { recursive: true });
  fs.writeFileSync(
    path.join(root, "src-tauri", "Cargo.toml"),
    `[package]\nname = "incfleetchat"\nversion = "0.0.0"\nedition = "2021"\n`,
  );
  fs.writeFileSync(
    path.join(root, "src-tauri", "tauri.conf.json"),
    JSON.stringify({ productName: "IncFleetChat", version: "0.0.0" }, null, 2) +
      "\n",
  );
}

describe("syncVersion", () => {
  let root: string;
  beforeEach(() => {
    root = fs.mkdtempSync(path.join(os.tmpdir(), "ifc-sync-"));
    writeFixture(root);
  });
  afterEach(() => {
    fs.rmSync(root, { recursive: true, force: true });
  });

  it("writes VERSION into package.json, Cargo.toml, and tauri.conf.json", () => {
    const v = syncVersion(root);
    expect(v).toBe("1.2.3-preview.1");
    expect(JSON.parse(fs.readFileSync(path.join(root, "package.json"), "utf8")).version).toBe(
      "1.2.3-preview.1",
    );
    expect(fs.readFileSync(path.join(root, "src-tauri", "Cargo.toml"), "utf8")).toMatch(
      /^version = "1\.2\.3-preview\.1"$/m,
    );
    expect(
      JSON.parse(fs.readFileSync(path.join(root, "src-tauri", "tauri.conf.json"), "utf8"))
        .version,
    ).toBe("1.2.3-preview.1");
  });

  it("throws when VERSION is empty", () => {
    fs.writeFileSync(path.join(root, "VERSION"), "  \n");
    expect(() => syncVersion(root)).toThrow(/VERSION/i);
  });
});
