# Releasing

Every product (Desqu, Screenbound, later Vials and Workspaces) is released on its own, by pushing a
tag named `<app>-v<version>`. A GitHub Actions workflow builds, signs and publishes it, and updates
that product's update feed. Installed apps then offer the update in their Settings.

## One-time setup (owner)

These cannot be done from the repo. Do them once.

1. **Updater signing keys.** One key pair per product was generated in `%USERPROFILE%\.tauri\`
   (`desqu.key`, `screenbound.key`, `vials.key`, `workspaces.key` plus `.key.pub`). The public
   halves are already in each app's `tauri.conf.json`. **Back the `.key` files up somewhere safe**:
   if one is lost, apps already installed can never be updated. They were created without a
   password.
2. **Repository secrets** (Settings → Secrets and variables → Actions, or the GitHub CLI):

   ```powershell
   gh secret set SCREENBOUND_UPDATER_KEY --repo Wolfskii/Desqu < "$HOME\.tauri\screenbound.key"
   gh secret set DESQU_UPDATER_KEY       --repo Wolfskii/Desqu < "$HOME\.tauri\desqu.key"
   ```

   Do the same for `VIALS_UPDATER_KEY` and `WORKSPACES_UPDATER_KEY` when those apps exist. If you ever
   give a key a password, also set `<PRODUCT>_UPDATER_KEY_PASSWORD`.
3. **GitHub Pages for the feeds.** The first release creates the `gh-pages` branch. After that, enable
   Pages once: Settings → Pages → *Deploy from a branch* → `gh-pages` / `(root)`, or

   ```powershell
   gh api -X POST repos/Wolfskii/Desqu/pages -f "source[branch]=gh-pages" -f "source[path]=/"
   ```

   Feeds then live at `https://wolfskii.github.io/Desqu/updates/<product>/latest.json`.

## One version for the family

Desqu, Screenbound, Vials, Workspaces and the browser extension share one version. A bump moves all of
them, and the checks fail if any file disagrees. Releasing one product means releasing the family at
that version: every product gets its own tag at the same number (`desqu-v0.3.0`, `screenbound-v0.3.0`,
`vials-v0.3.0`, `workspaces-v0.3.0`, `browser-v0.3.0`).

## Making a release

1. Pick the version and bump everything in one go. The script updates every product, the extension, the
   lockfiles and `Cargo.lock`, and moves each app's `## Unreleased` notes under the new version:

   ```powershell
   pwsh scripts/bump-version.ps1 -Version 0.3.0
   pwsh scripts/check-versions.ps1
   ```

2. Review each `apps/<app>/CHANGELOG.md`. Write the real notes under `## 0.3.0`; a product with nothing
   new still gets its one line. Those sections become the release notes and the in-app update text.
3. Commit and merge to `develop`, then tag the release and push:

   ```powershell
   git tag v0.3.0
   git push origin v0.3.0
   ```

   Or run the **Release** workflow by hand (Actions → Release → *Run workflow*) with the version.

The one release workflow then:

1. checks that every version file matches the tag (`scripts/check-versions.ps1`),
2. runs `task <app>:release:signed`, which builds the NSIS installer, the MSI and the portable build and
   signs the installers (`*.sig`),
3. writes `SHA256SUMS.txt`, `latest.json` and the release notes (`scripts/write-update-feed.ps1`),
4. creates the GitHub release with all of those attached (only **Desqu** is marked *Latest*, because
   GitHub has a single repository-wide "latest"),
5. publishes `latest.json` to `gh-pages` under `updates/<app>/`, leaving the other products' feeds
   untouched.

### Trying a build first

A version with a suffix, such as `screenbound-v0.2.0-rc.1`, publishes a GitHub **pre-release** and puts
its feed under `updates/<app>/prerelease/`, which no installed app reads. Testers can install it by
hand; nobody's updater moves. The version files must say `0.2.0-rc.1` for the check to pass
(`bump-version.ps1 -Version 0.2.0-rc.1`, which bumps every product).

## Building locally without CI

```powershell
task screenbound:release         # local unsigned build: installers + portable exe in dist/screenbound/
task screenbound:release:signed  # the same, signed with ~/.tauri/screenbound.key (press Enter at the password prompt)
task screenbound:deploy          # build (unsigned) then install it for the current user
pwsh scripts/write-update-feed.ps1 -App screenbound   # optional: dist/screenbound/feed/latest.json
```

If no key is found the build fails, because updater artifacts are switched on.

## What users see

- **Installed builds** get "Version X is available" in Settings (and in the tray menu), and install it
  with one click. The installer runs passively and the app restarts; running modules are stopped first
  so managed windows are restored.
- **Portable builds** show a *Download* button that opens the releases page. They never self-install.
- Updates are checked shortly after start and then daily (Settings → Updates can turn that off).

## Code signing and SmartScreen

The installers are not Authenticode-signed (no certificate). Windows SmartScreen may warn on first run
("Windows protected your PC": *More info → Run anyway*). The updater signature is separate: it
guarantees that an update was produced with the product's key, and tampered or wrongly signed updates
are refused.

## Undoing a release

Delete the GitHub release and tag, and revert the product's `latest.json` on `gh-pages` (or publish a
fixed version). Apps only move forward, so a bad release is fixed by releasing a higher version.
