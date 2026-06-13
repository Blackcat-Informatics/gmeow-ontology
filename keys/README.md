# GMEOW Release Keys

This directory holds the **public** release keys for the GMEOW ontology.  The
matching secret keys live in GitHub Actions secrets (`GMEOW_RELEASE_SIGNING_KEY`)
and are never committed.

## Files

| File | Purpose |
|------|---------|
| `gmeow-release-key.asc` | Armored OpenPGP Ed25519 public key used to sign `gmeow-full.gts` releases. |
| `*.secret.asc`, `*.secret` | Git-ignored secret-key material.  Do not commit. |

## Current release key

```text
EDEB4F7B306F94318E34BA794E2DD0CF66B26615
GMEOW Release Key <release@gmeow.blackcatinformatics.ca>
Algorithm: EDDSA (Ed25519)
```

The key fingerprint is also the GTS signing `kid` embedded in every signed
`gmeow-full.gts` file.

## Verifying a release snapshot

### Using `gmeow gts verify` (recommended)

```bash
pip install gmeow
python -m gmeow_tools.cli gts verify path/to/gmeow-full.gts
```

The CLI extracts the embedded transport public key, checks its fingerprint
against the committed `keys/gmeow-release-key.asc` if you pass
`--trusted-key`, and verifies every COSE signature in the file.

### Manual WoT cross-check

1. Extract the embedded public key:

   ```bash
   gmeow gts extract-key gmeow-full.gts -o /tmp/embedded.asc
   ```

2. Compare its fingerprint with the committed release key:

   ```bash
   gpg --show-keys /tmp/embedded.asc
   gpg --show-keys keys/gmeow-release-key.asc
   ```

3. Optionally check the release key's OpenPGP signatures on public keyservers
   or against a maintainer certification.

## Rotating the release key

1. Generate a new Ed25519 release key:

   ```bash
   gpg --batch --gen-key <<'EOF'
   Key-Type: EDDSA
   Key-Curve: ed25519
   Name-Real: GMEOW Release Key YYYY
   Name-Email: release@gmeow.blackcatinformatics.ca
   Expire-Date: 5y
   %no-protection
   %commit
   EOF
   ```

2. Export the **public** key to this directory:

   ```bash
   gpg --armor --export NEW_FINGERPRINT > keys/gmeow-release-key.asc
   ```

3. Store the **secret** key in the `GMEOW_RELEASE_SIGNING_KEY` GitHub secret.
   Do not commit the secret.

4. Update this README and `docs/key-management.md` with the new fingerprint.

5. Publish the new public key to the canonical locations (repository, website,
   keyservers) so consumers can verify older and newer releases.
