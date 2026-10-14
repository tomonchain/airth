# Releasing Airth

Pushing a version tag publishes Airth for macOS:

```sh
# 1. Bump the version in Cargo.toml (e.g. 0.2.0), then commit:
cargo build && git commit -am "Airth 0.2.0"
# 2. Tag and push:
git tag v0.2.0 && git push origin main v0.2.0
```

The **Release** workflow (`.github/workflows/release.yml`) then:

1. checks the tag matches `Cargo.toml`, runs the tests;
2. builds for Apple silicon and Intel, joined into one binary (`lipo`);
3. makes `Airth.app` (`airth release bundle`);
4. signs it with the Developer ID certificate and has Apple notarize it
   (skipped, with a warning, while the Apple secrets are not set);
5. publishes a GitHub release with:
   - `Airth-<version>.dmg`, to install (drag to Applications);
   - `Airth-<version>.zip`, the app the updater downloads;
   - `latest.json`, what running apps check.

## How apps update

A release build (built with `AIRTH_CHANNEL=release`) checks
`https://github.com/tomonchain/airth/releases/latest/download/latest.json`
every 30 minutes:

```json
{
  "version": "0.2.0",
  "notes": ["What changed", "..."],
  "url": "https://github.com/tomonchain/airth/releases/download/v0.2.0/Airth-0.2.0.zip",
  "sha256": "…",
  "signature": "…"
}
```

When the version is newer, the app downloads the zip and checks:

- its SHA-256;
- the Ed25519 signature of `airth-release <version> <sha256>`, against the
  public key built into Airth (`UPDATE_KEY` in `src/update.rs`);
- the app's code signature.

Then it shows "New update available" with the notes. "Restart to update"
swaps the whole `Airth.app` and keeps the previous one next to it as
`.Airth-previous.app` (`airth update --rollback` puts it back).

The signature binds the version to the archive: someone who can change the
release files (or the network) can neither serve a modified app, nor pass an
older signed app off as a newer one.

A build from a git checkout (`cargo install`, `airth service install`) keeps
updating from its checkout: build, test, install.

## Secrets

Set them in GitHub → Settings → Secrets and variables → Actions (or with
`gh secret set NAME`).

| Secret | What |
| --- | --- |
| `AIRTH_UPDATE_KEY` | **Required.** The private update signing key (hex). |
| `APPLE_CERTIFICATE_P12` | The *Developer ID Application* certificate and its key, exported as .p12, base64 (`base64 -i cert.p12`). |
| `APPLE_CERTIFICATE_PASSWORD` | The .p12 export password. |
| `APPLE_SIGNING_IDENTITY` | e.g. `Developer ID Application: Your Name (TEAMID)`. |
| `APPLE_API_KEY_P8` | An App Store Connect API key (.p8), base64, for notarization. |
| `APPLE_API_KEY_ID` | That key's ID. |
| `APPLE_API_ISSUER` | The issuer ID shown with App Store Connect API keys. |

Without the Apple secrets, releases still work, but macOS warns when the
app is first opened (right-click → Open). Updates work either way.

### The update signing key

Created once with `airth release keygen --out <file>`. It prints the public
key, which belongs in `UPDATE_KEY` in `src/update.rs`. Keep the private key
file out of the repository, and back it up somewhere safe: **losing it means
installed apps can no longer be updated** (they would need a manual
download of a version with a new key). If it leaks, make a new key, ship
it in a release signed with the old one, and remove the old one from GitHub.

### The Apple side (Apple Developer Program, $99/year)

1. Join the Apple Developer Program.
2. In Xcode → Settings → Accounts → Manage Certificates (or on
   developer.apple.com), create a **Developer ID Application** certificate.
   Export it from Keychain Access as .p12 with a password.
3. In App Store Connect → Users and Access → Integrations → App Store
   Connect API, create a key with the *Developer* role. Download the .p8
   (only once), note its key ID and the issuer ID.

## Trying a release before publishing it

`AIRTH_UPDATE_URL` (at build time, or when running) points the updater at
another manifest; archives may then also come from `http://127.0.0.1:…`:

```sh
AIRTH_CHANNEL=release AIRTH_UPDATE_URL=http://127.0.0.1:7690/latest.json cargo build --release
```

Serve a folder with a `latest.json` (made with `airth release sign`) and a
zipped `Airth.app` of a higher version, run the app from a bundle, and it
offers the update.
