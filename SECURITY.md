# Security

## Reporting a vulnerability

Please report suspected security issues to the maintainer listed in the
[README](README.md) rather than opening a public issue.

## Security advisories

### CVE-2026-55869 / CVE-2026-55868 — libsrt (SRT library)

The published Docker image bundles `libsrt` transitively via
`gstreamer1.0-plugins-bad`. Older published image digests shipped
`libsrt 1.5.4-1`, which predates the fixes for **CVE-2026-55869** and
**CVE-2026-55868**.

**Remediation:** the runtime stage is built from `debian:trixie-slim` and
installs `libsrt` without a version pin, so a fresh rebuild resolves the patched
`1.5.4-1+deb13u1` from `trixie-security` automatically. The `Dockerfile` now
asserts, at build time, that the installed `libsrt` is `>= 1.5.4-1+deb13u1` and
fails the build loudly otherwise.

To republish a patched image, a maintainer must trigger a fresh build. The
`Build and push Docker image to Docker Hub` workflow should be given
`workflow_dispatch` and a weekly `schedule` trigger (in addition to its existing
`release: published` trigger) so the published image keeps absorbing base-image
security updates without requiring a source release. That workflow change is
proposed alongside this file (see the associated pull request); it must be
applied by a maintainer because the automation token lacks the `workflow` OAuth
scope required to modify files under `.github/workflows/`.
