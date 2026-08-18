# Release integrity

Proof Lantern can publish unsigned preview binaries for Linux x86_64, macOS
Apple Silicon, macOS Intel, and Windows x86_64. Platform signing is deliberately
not a requirement for this small experimental open-source project.

The public release workflow:

1. builds locked release binaries with Rust 1.88 on matching GitHub-hosted
   runners;
2. packages each binary with the README and both licenses;
3. generates `SHA256SUMS` after every platform build succeeds;
4. creates a GitHub artifact attestation from those checksums; and
5. publishes the archives, checksums, and attestation from a version-matching
   `v*` tag.

Pull requests run the same platform build and packaging matrix without
publishing a release. Actions are pinned to full commit SHAs, checkout does not
persist credentials, and only the tag-only publishing job receives write and
attestation permissions.

## What users should expect

The binaries are not Apple-notarized or Windows Authenticode-signed, so macOS
Gatekeeper and Windows SmartScreen may show an unidentified-developer warning.
Checksums and provenance establish which public workflow produced the files,
but they do not replace operating-system publisher signing.

Before extracting an archive:

1. compare it with the matching line in `SHA256SUMS`; and
2. run `gh attestation verify <archive> --repo stoicpickle/prooflantern`.

Users who do not want to override an operating-system warning can continue to
build the locked source release:

```sh
cargo install --locked --git https://github.com/stoicpickle/prooflantern.git
```

Platform signing can be added later if maintainers intentionally acquire and
configure the necessary identities.
