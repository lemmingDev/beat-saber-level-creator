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
