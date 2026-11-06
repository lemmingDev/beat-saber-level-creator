# Code signing

The desktop builds are unsigned by default. Unsigned apps work, but:

| Platform | Symptom without signing |
|---|---|
| macOS | Gatekeeper refuses to open: *"Beatmapper is damaged and can't be opened"*. Users must right-click → Open, or run `xattr -dr com.apple.quarantine /Applications/Beatmapper.app` |
| Windows | SmartScreen shows *"Windows protected your PC"* until enough people have run the file |

Signing is configured entirely through repository secrets. If a secret group is
absent the workflow still succeeds and produces an unsigned build.

---

## macOS — Developer ID signing + notarisation

### What you need

1. An **Apple Developer Program** membership (99 USD/year).
2. A **Developer ID Application** certificate (created in the Apple Developer
   portal, then exported from Keychain Access as a `.p12`).
3. An **app-specific password** for your Apple ID
   (appleid.apple.com → Sign-In and Security → App-Specific Passwords).
4. Your **Team ID** (10 characters, shown in the Developer portal).

### Creating the `APPLE_CERTIFICATE` secret

```bash
# Export "Developer ID Application: Your Name (TEAMID)" from Keychain Access
# as a .p12 with a password, then:
base64 -i Certificates.p12 | pbcopy     # now on the clipboard
```

Paste that into a repository secret named `APPLE_CERTIFICATE`.

### Secrets to add

| Secret | Value |
|---|---|
| `APPLE_CERTIFICATE` | base64 of the `.p12` |
| `APPLE_CERTIFICATE_PASSWORD` | the password used when exporting the `.p12` |
| `APPLE_SIGNING_IDENTITY` | e.g. `Developer ID Application: Your Name (ABCDE12345)` |
| `APPLE_ID` | your Apple ID email |
| `APPLE_PASSWORD` | the app-specific password (not your Apple ID password) |
| `APPLE_TEAM_ID` | e.g. `ABCDE12345` |

Tauri signs the `.app`, notarises it with Apple, and staples the ticket to the
`.dmg` automatically. No extra workflow steps are required.

### Verify a build

```bash
codesign --verify --deep --strict --verbose=2 /Applications/Beatmapper.app
spctl --assess --type execute --verbose=2 /Applications/Beatmapper.app
xcrun stapler validate Beatmapper_0.1.0_aarch64.dmg
```

---

## Windows — Authenticode signing

The workflow uses **Azure Trusted Signing** (formerly Azure Code Signing)
because it is the only option that works cleanly from CI without a physical
HSM token.

### What you need

1. An **Azure subscription**.
2. A **Trusted Signing account** with **Identity Validation** completed
   (individual or organisation — approval can take a few days).
3. A **certificate profile** inside that account.
4. An **Entra ID app registration** (service principal) granted the
   *Trusted Signing Certificate Profile Signer* role on the account.

### Secrets to add

| Secret | Value |
|---|---|
| `AZURE_TENANT_ID` | Entra ID tenant id |
| `AZURE_CLIENT_ID` | app registration (client) id |
| `AZURE_CLIENT_SECRET` | client secret value |
| `AZURE_TS_ENDPOINT` | e.g. `https://eus.codesigning.azure.net` |
| `AZURE_TS_ACCOUNT` | Trusted Signing account name |
| `AZURE_TS_PROFILE` | certificate profile name |

When `AZURE_TS_ENDPOINT` is set, the workflow installs `trusted-signing-cli`
and patches `src-tauri/tauri.conf.json` on the Windows runner so Tauri signs
the `.msi` and `.exe` with Authenticode + timestamping.

### Alternatives

A traditional OV/EV certificate also works, but since June 2023 these must live
on a hardware token or cloud HSM, so they cannot simply be uploaded as a
secret. EV certificates additionally grant SmartScreen reputation immediately;
OV certificates have to earn it over time.

---

## Linux

There is no code-signing requirement for the `.deb` or `.AppImage`. If you
publish an APT repository later you would add a GPG key instead.

---

# Free options

## macOS — ad-hoc signing (free)

The workflow signs ad-hoc when `APPLE_SIGNING_IDENTITY` is unset, by falling
back to `-`. This replaces the alarming *"Beatmapper is damaged and can't be
opened"* dialog with the milder *"unidentified developer"* one, which users can
dismiss with right-click → **Open**.

Users can also clear the quarantine flag:

```bash
xattr -dr com.apple.quarantine /Applications/Beatmapper.app
```

This is free but is **not** notarisation: it will never be a warning-free,
double-clickable `.dmg`. Only a paid Developer ID plus notarisation gives that.

Apple does run a **fee waiver** programme for non-profits, accredited
educational institutions and government entities — worth applying for if the
project qualifies.

## Windows — free signing via SignPath (wired up)

The repository is public, so it qualifies for the free SignPath Foundation
programme. `.github/workflows/signpath-windows.yml` is ready and stays dormant
until you enable it.

### One-time setup

1. Apply at <https://signpath.org/> (free for open-source projects, manual approval).
2. In the SignPath portal create:
   - an **Organisation**
   - a **Project** linked to this GitHub repo
   - a **Signing Policy** (e.g. `release-signing`)
   - an **Artifact Configuration** whose root element is `<zip-file>` and which
     contains `*.msi` and `*.exe`
3. Install the **SignPath GitHub App** on this repository.
4. Add secrets / variables under *Settings → Secrets and variables → Actions*:

   | Kind | Name | Value |
   |---|---|---|
   | Secret | `SIGNPATH_API_TOKEN` | API token with submitter rights |
   | Variable | `SIGNPATH_ORGANIZATION_ID` | organisation GUID |
   | Variable | `SIGNPATH_PROJECT_SLUG` | e.g. `beatmapper` |
   | Variable | `SIGNPATH_SIGNING_POLICY_SLUG` | e.g. `release-signing` |
   | Variable | `SIGNPATH_ARTIFACT_CONFIG_SLUG` | e.g. `windows-installers` |
   | Variable | `SIGNPATH_ENABLED` | `true` |

5. Push a tag. The job builds Windows, has the installers signed, and replaces
   the unsigned `.msi`/`.exe` on the release with signed ones.

## Windows — free signing for open-source projects

Two services sign open-source binaries for free with a real, trusted
certificate:

| Service | Notes |
|---|---|
| **SignPath Foundation** | Free for OSS; integrate via their GitHub Action |
| **Certum Open Source Code Signing** | Free certificate for OSS, issued via SimplySign cloud |

Both require the project to be **public** and openly licensed. If this fork is
ever made public, SignPath is the path of least resistance: their action signs
the `.msi`/`.exe` and the SmartScreen prompt disappears as reputation builds.

A **self-signed** certificate is free but useless for distribution — SmartScreen
still warns, and users would have to install your root certificate first.

## Distributing without any signing

Builds from source never trigger a trust prompt:

```bash
git clone https://github.com/lemmingDev/beat-saber-level-creator.git
cd beat-saber-level-creator
pnpm install
pnpm exec panda codegen --clean
pnpm tauri build
```

Requires Node 22+, pnpm 11, Rust stable with the `wasm32-unknown-unknown`
target, and `wasm-pack`.
