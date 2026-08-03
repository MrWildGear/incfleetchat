# Version, Release Pipeline & Updater Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Single-source `VERSION` sync, Windows draft GitHub releases (cliff + NSIS + portable), and Design C in-app updater so installed users update without reinstall.

**Architecture:** Root `VERSION` + `scripts/sync-version.mjs` keep npm/Cargo/tauri configs aligned. CI on the `release` branch builds a draft Windows release via `tauri-action` (updater JSON + signing). The React main window shows `getVersion()`, checks updates on launch, and surfaces Design C modals + Settings About.

**Tech Stack:** Tauri v2, `@tauri-apps/plugin-updater`, `@tauri-apps/plugin-process`, GitHub Actions (`tauri-action`, `git-cliff`), Node ESM sync script, Vitest, React.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-08-03-version-release-updater-design.md`
- Version source: root `VERSION` (one-line semver); sync to `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`
- Runtime display: Tauri `getVersion()`, not importing `VERSION`
- Release: `release` branch + `workflow_dispatch`; `releaseDraft: true`; Windows only; NSIS only; portable from `src-tauri/target/release/*.exe`
- Updater endpoint: `https://github.com/MrWildGear/incfleetchat/releases/latest/download/latest.json`
- UI: Design C only (header `vX.Y.Z`, launch modal, progress modal, Settings About, Settings gear accent dot after Later)
- No macOS/Linux CI (stubs already in `docs/future-plans/`)
- No in-app release notes / skip-version / MSI updater path
- TDD for sync script + update UX helpers; Conventional Commits

## File map

| File | Responsibility |
|------|----------------|
| `VERSION` | Single source of truth (one line) |
| `scripts/sync-version.mjs` | Read `VERSION`, write three config files |
| `scripts/sync-version.test.ts` | Vitest: sync against temp fixtures |
| `package.json` | `version` field + `sync-version` script |
| `src-tauri/Cargo.toml` | `[package].version` |
| `src-tauri/tauri.conf.json` | `version`, NSIS target, updater plugin config, `createUpdaterArtifacts` |
| `src-tauri/capabilities/default.json` | `updater:default`, process relaunch permission |
| `src-tauri/src/commands.rs` | Register updater + process plugins |
| `src-tauri/Cargo.toml` | `tauri-plugin-updater`, `tauri-plugin-process` deps |
| `cliff.toml` | Conventional-commit release notes |
| `.github/workflows/release.yml` | Changelog + Windows publish + portable upload |
| `src/lib/updateUx.ts` | Pure helpers for check/progress UI state |
| `src/lib/updateUx.test.ts` | Vitest for helpers |
| `src/components/UpdateModals.tsx` | Launch + progress modals |
| `src/components/SettingsPanel.tsx` | About footer + Check for updates |
| `src/App.tsx` | Header version, gear dot, launch check, wire modals |
| `src/hooks/useAppUpdater.ts` | Orchestrate check / install / relaunch (thin) |
| `README.md` | Version bump, release, signing, install vs portable |

---

### Task 1: VERSION + sync-version (TDD)

**Files:**
- Create: `VERSION`
- Create: `scripts/sync-version.mjs`
- Create: `scripts/sync-version.test.ts`
- Modify: `package.json` (add script; version stays in sync via script)
- Modify: `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json` (after first sync)

**Interfaces:**
- Consumes: `VERSION` file contents (trimmed string)
- Produces: `syncVersion(rootDir: string): string` — returns the version written; mutates the three files under `rootDir`

- [ ] **Step 1: Write the failing test**

Create `scripts/sync-version.test.ts`:

```ts
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run scripts/sync-version.test.ts`

Expected: FAIL (module / `syncVersion` missing)

- [ ] **Step 3: Implement sync script**

Create `scripts/sync-version.mjs`:

```js
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
```

Create root `VERSION` with current app version:

```text
0.1.0
```

Add to `package.json` scripts:

```json
"sync-version": "node scripts/sync-version.mjs"
```

- [ ] **Step 4: Run tests and sync**

Run: `npx vitest run scripts/sync-version.test.ts`  
Expected: PASS

Run: `npm run sync-version`  
Expected: `Synced version 0.1.0` and three files show `0.1.0`

- [ ] **Step 5: Commit**

```bash
git add VERSION scripts/sync-version.mjs scripts/sync-version.test.ts package.json src-tauri/Cargo.toml src-tauri/tauri.conf.json package-lock.json
git commit -m "feat(release): add VERSION file and sync-version script"
```

---

### Task 2: Header version via getVersion

**Files:**
- Modify: `src/App.tsx`

**Interfaces:**
- Consumes: `getVersion()` from `@tauri-apps/api/app` → `Promise<string>`
- Produces: header shows `v{version}` when loaded

- [ ] **Step 1: Add version state and display**

In `src/App.tsx`, import `getVersion` from `@tauri-apps/api/app`. Add `const [appVersion, setAppVersion] = useState<string | null>(null);` and in a `useEffect` after mount:

```ts
void getVersion()
  .then(setAppVersion)
  .catch(() => setAppVersion(null));
```

Change the title row to:

```tsx
<h1 className="text-sm font-semibold tracking-tight">
  IncFleetChat
  {appVersion ? (
    <span className="ml-1.5 text-[10px] font-normal tabular-nums text-muted">
      v{appVersion}
    </span>
  ) : null}
</h1>
```

Do **not** change the Tools header.

- [ ] **Step 2: Manual check in `npm run tauri dev`**

Expected: header shows `IncFleetChat v0.1.0` (or current synced version).

- [ ] **Step 3: Commit**

```bash
git add src/App.tsx
git commit -m "feat(ui): show app version in main header"
```

---

### Task 3: Update UX pure helpers (TDD)

**Files:**
- Create: `src/lib/updateUx.ts`
- Create: `src/lib/updateUx.test.ts`

**Interfaces:**
- Produces:
  - `formatAppVersionLabel(version: string): string` — always one leading `v`
  - `type ManualCheckOutcome = { kind: "upToDate" } | { kind: "available"; version: string } | { kind: "error"; message: string }`
  - `manualCheckOutcome(update: { version: string } | null, err?: unknown): ManualCheckOutcome`
  - `type ProgressStage = "downloading" | "installing"`
  - `progressStageFromUpdaterEvent(event: "Started" | "Progress" | "Finished"): ProgressStage` — `Finished` ⇒ `"installing"`; else `"downloading"`

- [ ] **Step 1: Write failing tests**

Create `src/lib/updateUx.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import {
  formatAppVersionLabel,
  manualCheckOutcome,
  progressStageFromUpdaterEvent,
} from "./updateUx";

describe("formatAppVersionLabel", () => {
  it("prefixes v when missing", () => {
    expect(formatAppVersionLabel("0.1.0")).toBe("v0.1.0");
  });
  it("does not double-prefix", () => {
    expect(formatAppVersionLabel("v0.1.0")).toBe("v0.1.0");
  });
});

describe("manualCheckOutcome", () => {
  it("maps null update to upToDate", () => {
    expect(manualCheckOutcome(null)).toEqual({ kind: "upToDate" });
  });
  it("maps update object to available", () => {
    expect(manualCheckOutcome({ version: "0.2.0" })).toEqual({
      kind: "available",
      version: "0.2.0",
    });
  });
  it("maps thrown error to error message", () => {
    expect(manualCheckOutcome(null, new Error("network down"))).toEqual({
      kind: "error",
      message: "network down",
    });
  });
});

describe("progressStageFromUpdaterEvent", () => {
  it("treats Started and Progress as downloading", () => {
    expect(progressStageFromUpdaterEvent("Started")).toBe("downloading");
    expect(progressStageFromUpdaterEvent("Progress")).toBe("downloading");
  });
  it("treats Finished as installing", () => {
    expect(progressStageFromUpdaterEvent("Finished")).toBe("installing");
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/lib/updateUx.test.ts`  
Expected: FAIL (module missing)

- [ ] **Step 3: Implement helpers**

Create `src/lib/updateUx.ts` implementing the interfaces above. For `manualCheckOutcome`, if `err` is defined, prefer error over update; stringify non-Error as `String(err)` or `"Update check failed"`.

- [ ] **Step 4: Run tests**

Run: `npx vitest run src/lib/updateUx.test.ts`  
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/lib/updateUx.ts src/lib/updateUx.test.ts
git commit -m "feat(updater): add pure update UX helpers"
```

---

### Task 4: Tauri updater + process plugins and config

**Files:**
- Modify: `package.json` / lockfile (npm packages)
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/commands.rs` (plugin registration near existing `.plugin` calls)
- Modify: `src-tauri/tauri.conf.json`
- Modify: `src-tauri/capabilities/default.json`

**Interfaces:**
- Consumes: signing **public** key string (generated locally; see steps)
- Produces: configured updater plugin + permissions so frontend `check` / `downloadAndInstall` / `relaunch` work

- [ ] **Step 1: Install plugins**

Run:

```bash
npm run tauri add updater
npm run tauri add process
```

If the CLI does not fully wire files, manually ensure:

- npm: `@tauri-apps/plugin-updater`, `@tauri-apps/plugin-process`
- Cargo: `tauri-plugin-updater`, `tauri-plugin-process` (desktop/`cfg` as CLI suggests)
- `commands.rs` registers:

```rust
.plugin(tauri_plugin_updater::Builder::new().build())
.plugin(tauri_plugin_process::init())
```

(alongside existing opener / window-state plugins)

- [ ] **Step 2: Generate signing keys (human + agent)**

Run (developer machine; do **not** commit private key):

```bash
npm run tauri signer generate -- -w "$HOME/.tauri/incfleetchat.key"
```

On Windows PowerShell, use a path under the user profile, e.g. `$env:USERPROFILE\.tauri\incfleetchat.key`.

Copy the **public** key contents into `tauri.conf.json` → `plugins.updater.pubkey`.  
Store private key (+ password if set) for later GitHub Actions secrets: `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.

- [ ] **Step 3: Configure bundle + updater endpoint**

In `src-tauri/tauri.conf.json`, ensure:

```json
"bundle": {
  "active": true,
  "targets": ["nsis"],
  "createUpdaterArtifacts": true,
  "icon": [ "...existing icons..." ]
},
"plugins": {
  "updater": {
    "pubkey": "<PASTE_PUBLIC_KEY>",
    "endpoints": [
      "https://github.com/MrWildGear/incfleetchat/releases/latest/download/latest.json"
    ],
    "windows": {
      "installMode": "passive"
    }
  }
}
```

Keep existing `icon` array values; only change targets / updater fields as needed.

- [ ] **Step 4: Capabilities**

In `src-tauri/capabilities/default.json`, add permissions:

```json
"updater:default",
"process:allow-relaunch"
```

(If the process plugin documents `process:default` instead, use that — prefer the minimal permission that allows `relaunch`.)

- [ ] **Step 5: Verify app still boots**

Run: `npm run tauri dev`  
Expected: app starts; header still shows version. Update check may fail quietly until a published release exists.

- [ ] **Step 6: Commit**

```bash
git add package.json package-lock.json src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/commands.rs src-tauri/tauri.conf.json src-tauri/capabilities/default.json
git commit -m "feat(updater): wire Tauri updater and process plugins"
```

Do **not** commit private keys or `.env` with secrets.

---

### Task 5: Updater hook + Design C UI

**Files:**
- Create: `src/hooks/useAppUpdater.ts`
- Create: `src/components/UpdateModals.tsx`
- Modify: `src/App.tsx`
- Modify: `src/components/SettingsPanel.tsx`

**Interfaces:**
- Consumes: `check`, `downloadAndInstall` from `@tauri-apps/plugin-updater`; `relaunch` from `@tauri-apps/plugin-process`; helpers from `src/lib/updateUx.ts`
- Produces: `useAppUpdater()` returning roughly:

```ts
{
  appVersion: string | null;
  pendingVersion: string | null;       // remote version when available
  promptOpen: boolean;                 // launch / available modal
  progress: null | { version: string; stage: "downloading" | "installing" };
  progressError: string | null;
  deferredUpdate: boolean;             // Later was chosen; Settings dot
  manualResult: ManualCheckOutcome | null;
  checkingManual: boolean;
  onUpdateNow: () => Promise<void>;
  onLater: () => void;
  onCloseProgressError: () => void;
  onRetryInstall: () => Promise<void>;
  checkManual: () => Promise<void>;
  runLaunchCheck: () => Promise<void>; // silent on error
}
```

- [ ] **Step 1: Implement `useAppUpdater`**

- `runLaunchCheck`: `const update = await check();` — on success with update, set `pendingVersion` + `promptOpen=true`; on throw, swallow.
- `checkManual`: set `checkingManual`; try/catch; set `manualResult` via `manualCheckOutcome`; if available, also set `pendingVersion` + `promptOpen=true`.
- `onLater`: close prompt; set `deferredUpdate=true`.
- `onUpdateNow` / `onRetryInstall`: close prompt; set progress downloading; `await update.downloadAndInstall((e) => { set stage via progressStageFromUpdaterEvent(e.event) })` then `await relaunch()`; on failure set `progressError`.
- Keep a ref to the last `Update` object from `check()` for install.

- [ ] **Step 2: Implement `UpdateModals.tsx`**

Small `max-w-xs` centered panels (same scrim pattern as Settings: `fixed inset-0 z-50 bg-black/60`):

1. **Prompt** when `promptOpen && !progress`: title `Update available`, body `{version} is ready.`, buttons **Update now** / **Later**.
2. **Progress** when `progress`: heading `Updating…` or `Installing…`, muted version, indeterminate 2px accent bar, no cancel after install starts (omit Cancel for YAGNI unless already trivial).
3. **Error** when `progressError`: `Update failed`, message, **Retry** / **Close**.

- [ ] **Step 3: Wire `App.tsx`**

- Use hook; call `runLaunchCheck` once after hydrate (or when settings loaded).
- Header version: use `appVersion` from hook (or keep local `getVersion` — prefer one source in the hook).
- Settings button: relative wrapper; if `deferredUpdate && pendingVersion`, show 4px accent dot on the gear.
- Render `<UpdateModals ... />`.

- [ ] **Step 4: Settings About footer**

Above Close in `SettingsPanel.tsx`:

```tsx
<div className="mt-6 border-t border-border pt-4">
  <p className="text-xs uppercase tracking-wide text-muted">About</p>
  <p className="mt-1 text-sm text-fg">
    IncFleetChat {appVersion ? formatAppVersionLabel(appVersion) : "…"}
  </p>
  <button
    type="button"
    disabled={checkingManual}
    className="mt-2 w-full rounded-md border border-border px-3 py-1.5 text-xs text-muted hover:text-fg disabled:opacity-50"
    onClick={() => void checkManual()}
  >
    {checkingManual ? "Checking…" : "Check for updates"}
  </button>
  {manualResult?.kind === "upToDate" && (
    <p className="mt-1 text-[10px] text-muted">Up to date</p>
  )}
  {manualResult?.kind === "available" && (
    <p className="mt-1 text-[10px] text-muted">
      {formatAppVersionLabel(manualResult.version)} available
    </p>
  )}
  {manualResult?.kind === "error" && (
    <p className="mt-1 text-[10px] text-overdue">{manualResult.message}</p>
  )}
</div>
```

Pass `appVersion`, `checkManual`, `checkingManual`, `manualResult` from App into SettingsPanel props (extend props; avoid reaching into updater from Tools).

- [ ] **Step 5: Smoke in `tauri dev`**

- Header version visible.
- Settings About + Check for updates shows error or up to date (no published release yet is OK).
- No crash if check fails.

- [ ] **Step 6: Commit**

```bash
git add src/hooks/useAppUpdater.ts src/components/UpdateModals.tsx src/App.tsx src/components/SettingsPanel.tsx
git commit -m "feat(updater): add Design C update prompt and Settings check"
```

---

### Task 6: cliff.toml + release workflow

**Files:**
- Create: `cliff.toml`
- Create: `.github/workflows/release.yml`

**Interfaces:**
- Consumes: conventional commits; synced `tauri.conf.json` version; signing secrets
- Produces: draft GitHub release with notes, NSIS (+ updater artifacts via tauri-action), portable exe

- [ ] **Step 1: Add `cliff.toml`**

Adapt eve-wrench config; keep Features / Bug Fixes / etc.; product-agnostic body (no hard-coded app name in template groups):

```toml
# git-cliff — conventional commits → release notes
# https://git-cliff.org

[changelog]
header = ""
body = """
{% for group, commits in commits | group_by(attribute="group") %}
### {{ group | upper_first }}
{% for commit in commits %}\
- {{ commit.message | split(pat=": ") | last | trim | upper_first }}
{% endfor %}\
{% endfor %}"""
trim = true
footer = ""

[git]
conventional_commits = true
filter_unconventional = true
filter_commits = true
tag_pattern = "v[0-9]*"
sort_commits = "oldest"
commit_parsers = [
  { message = "^feat", group = "Features" },
  { message = "^fix", group = "Bug Fixes" },
  { message = "^perf", group = "Performance" },
  { message = "^refactor", group = "Refactor" },
  { message = "^docs", group = "Documentation" },
  { message = "^ci", group = "Build & CI" },
  { message = "^chore", skip = true },
  { message = "^test", skip = true },
  { message = "^style", skip = true },
  { message = ".*", skip = true },
]
```

- [ ] **Step 2: Add `.github/workflows/release.yml`**

```yaml
name: "Release"

on:
  workflow_dispatch:
  push:
    branches:
      - release

jobs:
  changelog:
    runs-on: ubuntu-latest
    outputs:
      notes: ${{ steps.notes.outputs.notes }}
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - name: Generate changelog
        id: cliff
        uses: orhun/git-cliff-action@v4
        with:
          config: cliff.toml
          args: --unreleased --strip header

      - name: Prepare notes output
        id: notes
        shell: bash
        env:
          CONTENT: ${{ steps.cliff.outputs.content }}
        run: |
          notes="## What's Changed"$'\n\n'"$CONTENT"
          if [ -z "$(printf '%s' "$CONTENT" | tr -d '[:space:]')" ]; then
            notes="See the assets below to download and install IncFleetChat."
          fi
          {
            echo 'notes<<CHANGELOG_EOF'
            echo "$notes"
            echo CHANGELOG_EOF
          } >> "$GITHUB_OUTPUT"

  publish-tauri:
    needs: changelog
    permissions:
      contents: write
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4

      - name: Setup Node
        uses: actions/setup-node@v4
        with:
          node-version: lts/*

      - name: Install Rust stable
        uses: dtolnay/rust-toolchain@stable

      - name: Rust cache
        uses: swatinem/rust-cache@v2
        with:
          workspaces: "./src-tauri -> target"

      - name: Install frontend dependencies
        run: npm install

      - name: Sync version from VERSION
        run: npm run sync-version

      - name: Detect pre-release
        id: version
        shell: bash
        run: echo "prerelease=$(node -p "require('./src-tauri/tauri.conf.json').version.includes('-')")" >> "$GITHUB_OUTPUT"

      - uses: tauri-apps/tauri-action@v0
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
          TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}
        with:
          tagName: v__VERSION__
          releaseName: "IncFleetChat v__VERSION__"
          releaseBody: ${{ needs.changelog.outputs.notes }}
          releaseDraft: true
          prerelease: ${{ steps.version.outputs.prerelease }}
          includeUpdaterJson: true

      - name: Upload portable Windows executable
        shell: bash
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: |
          version=$(node -p "require('./src-tauri/tauri.conf.json').version")
          exe=$(find src-tauri/target/release -maxdepth 1 -name '*.exe' | head -1)
          if [ -z "$exe" ]; then
            echo "Portable exe not found under src-tauri/target/release" >&2
            exit 1
          fi
          dest="IncFleetChat_${version}_x64-portable.exe"
          cp "$exe" "$dest"
          gh release upload "v${version}" "$dest" --clobber
```

- [ ] **Step 3: Commit**

```bash
git add cliff.toml .github/workflows/release.yml
git commit -m "ci(release): add Windows draft release workflow and git-cliff"
```

---

### Task 7: GitHub repo, secrets, README

**Files:**
- Modify: `README.md`
- Ops: create public repo + secrets (not files)

- [ ] **Step 1: Create public repo and push**

```bash
gh repo create MrWildGear/incfleetchat --public --source=. --remote=origin --push
```

If the repo already exists empty, add `origin` and push `master` instead.

- [ ] **Step 2: Add Actions secrets**

In GitHub → Settings → Secrets → Actions:

- `TAURI_SIGNING_PRIVATE_KEY` — full private key contents (or path contents as CI expects)
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — password if the key was generated with one; otherwise empty string secret or omit if action allows

- [ ] **Step 3: Document in README**

Append sections:

```markdown
## Version

Edit the root `VERSION` file, then run:

```bash
npm run sync-version
```

Commit the updated `VERSION` and synced config files.

## Release (Windows)

1. Merge work to `master`, bump `VERSION`, sync, commit.
2. Push `master` to `origin`, then update the `release` branch to that commit (or merge) and push `release`.
3. Wait for the Release workflow; open the **draft** GitHub release; verify NSIS, `.sig`, `latest.json`, and `IncFleetChat_*_x64-portable.exe`.
4. **Publish** the release when ready. Installed (NSIS) apps then see updates via in-app updater.
5. Portable builds do **not** auto-update — download a new portable exe or use the NSIS install for updates.

One-time: generate signing keys with `npm run tauri signer generate`, put the public key in `tauri.conf.json`, and store the private key in Actions secrets (see design spec).
```

- [ ] **Step 4: Commit README**

```bash
git add README.md
git commit -m "docs: document version sync and Windows release/updater"
git push origin master
```

- [ ] **Step 5: Manual release smoke (when ready)**

1. Ensure secrets set; push `release` branch or run workflow_dispatch.
2. Confirm draft assets.
3. Publish; install NSIS build on a machine; bump version; cut another release; confirm in-app update.

---

## Self-review (plan vs spec)

| Spec requirement | Task |
|------------------|------|
| `VERSION` + sync script | Task 1 |
| Header version via `getVersion` | Task 2 |
| Pure update UX tests | Task 3 |
| Updater plugins, pubkey, endpoint, NSIS, artifacts | Task 4 |
| Design C UI + launch/manual check | Task 5 |
| cliff + release.yml + portable path | Task 6 |
| Public repo + secrets + docs | Task 7 |
| macOS/Linux deferred stubs | Already present (`docs/future-plans/`) |
| Draft then Publish for updater | Task 6 + README |
| Prerelease hyphen / `/latest` skips prereleases | Task 6 detect step + README/spec note |

No TBD placeholders in task steps. Helper names consistent across Tasks 3–5.
