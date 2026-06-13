# GMEOW Release Key Management

## Goal

Every published `gmeow-full.gts` snapshot is cryptographically signed so that
consumers can verify its integrity and authenticate its origin.  The signing
system is intentionally simple and offline-first:

* The **release key** is an OpenPGP Ed25519 certificate.
* The **public** certificate is committed in `keys/gmeow-release-key.asc` and
  embedded in the first `meta` frame of every signed `.gts` file as the
  **transport key**.
* The **secret** key is stored only in the `GMEOW_RELEASE_SIGNING_KEY` GitHub
  Actions secret and is never present in the repository.

## Trust model

1. **Default verification** — `gmeow gts verify` trusts the transport key that
   the file itself carries.  This proves the file has not been modified since
   it left the release signing process, but it does not bind the file to a
   real-world identity.

2. **Repository-pin verification** — Pass `--trusted-key keys/gmeow-release-key.asc`
   to verify the file against the public key committed in this repository.  This
   binds the snapshot to the repository's release key.

3. **Web-of-Trust cross-check** — An agent that wants stronger assurance can:

   1. Extract the embedded public key with `gmeow gts extract-key`.
   2. Compare its fingerprint to `keys/gmeow-release-key.asc`.
   3. Check the release key's OpenPGP signatures (self-signature, maintainer
      certifications, keyserver attestations) using `gpg`.

## Verification commands

```bash
# Default: verify against the embedded transport key
gmeow gts verify dist/gmeow-full.gts

# Pin to the repository's public release key
gmeow gts verify dist/gmeow-full.gts --trusted-key keys/gmeow-release-key.asc

# Inspect file metadata without running signature verification
gmeow gts info --no-verify dist/gmeow-full.gts

# Extract the embedded public key for manual WoT checks
gmeow gts extract-key dist/gmeow-full.gts -o /tmp/embedded.asc
```

## Current release key

```text
EDEB4F7B306F94318E34BA794E2DD0CF66B26615
GMEOW Release Key <release@gmeow.blackcatinformatics.ca>
Algorithm: EDDSA (Ed25519)
```

## Human-friendly key identifiers

To make key comparison easier for humans, `gmeow gts verify` prints:

* The OpenPGP fingerprint (40 hex characters).
* An **emojihash** — a deterministic sequence of emojis derived from the raw
  Ed25519 public key bytes.
* **Randomart** — OpenSSH-style ASCII art fingerprint.

These visual hashes are computed locally from the public key; they are not part
of the signature and do not affect verification.

## Release automation

Both `.github/workflows/release.yml` and `.github/workflows/pypi-publish-gmeow.yml`
sign the snapshot before uploading artifacts:

```yaml
- run: uv run gmeow gts compile-full \
         --sign-key /tmp/gpg/signing-key.asc \
         --public-key keys/gmeow-release-key.asc \
         -o generated/dist/gmeow-full.gts
- run: uv run gmeow gts verify generated/dist/gmeow-full.gts
```

The committed `generated/dist/gmeow-full.gts` remains **unsigned**; only the
release artifacts are signed.

## Key rotation

See `keys/README.md` for the rotation procedure.
