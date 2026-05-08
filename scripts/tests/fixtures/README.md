# Test fixtures for `scripts/test-fetch-yt-dlp.bats`

These files are committed test fixtures used by the hermetic bats suite that
exercises `scripts/fetch-yt-dlp.sh` without network access. The bats
`setup_file` provisions a stub-`curl` PATH shim that emits these bytes
instead of fetching from the upstream yt-dlp release URL.

| File                  | Purpose                                                                 |
|-----------------------|-------------------------------------------------------------------------|
| `valid-binary.bin`    | Deterministic byte string used as the "fetched" binary in success tests |
| `tampered-binary.bin` | Different bytes — its SHA does NOT match `valid-sha256sums`             |
| `valid-sha256sums`    | SHA2-256SUMS-shaped file listing `valid-binary.bin`'s SHA               |
| `valid-sig`           | Detached GPG signature on `valid-sha256sums` made with the test key    |
| `test-signer.asc`     | Public key of the test signer (record-keeping; not used at test time)   |
| `wrong-key.asc`       | A different test key — imported in the GPG-failure test path           |

## Why two keys?

The hermetic GPG-failure test imports `wrong-key.asc` as the "public key"
the script trusts, then asks `gpg --verify` to check `valid-sig` (which was
signed by the **test signer**, not the wrong-key). Verification fails with
"No public key" / "BAD signature" — exactly what we want the production code
path to surface as exit code 74.

This is more robust than corrupting the signature file (which would still
parse) and lets us reuse the same `valid-sig` across both success and
failure tests.

## Regenerating

If a fixture is corrupted or the GPG keys' validity expires, regenerate
all fixtures by running:

```sh
bash scripts/tests/fixtures/generate-fixtures.sh
```

Every run produces fresh keys with fresh fingerprints, so signatures and
`.asc` files change byte-for-byte. The bats tests do not pin fingerprints
— they only care about the success / failure outcome of `gpg --verify`.

## Why fixture-bytes-not-real-yt-dlp?

Two reasons:

1. **Hermeticity.** Real yt-dlp binaries are 15+ MB and embedded across
   target triples; committing them would balloon the repo. Fixture bytes
   are 30 bytes each and version-controlled cleanly.
2. **Layered testing.** The bats suite proves the script's logic
   (argv parsing, target-triple → asset map, SHA verify, GPG verify) works.
   The local `dist build --artifacts=local` smoke (`AC #7`) plus CI's
   release-tag run prove the script works against real upstream artifacts.
   Splitting unit-vs-integration this way keeps each test layer fast and
   focused.
