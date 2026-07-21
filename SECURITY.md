# Security policy

## Supported versions

VectorKit has not published a supported binary release yet. Security fixes are
prepared against the latest repository revision and will be assigned to a
supported release line when `v0.1.0` is published.

## Reporting a vulnerability

Please use [GitHub's private vulnerability reporting](https://github.com/gungorbasa/VectorKit/security/advisories/new).
Do not open a public issue for suspected vulnerabilities or include sensitive
application data, private corpora, credentials, or device captures.

Include the affected revision, platform, package, minimal reproduction, impact,
and any known mitigations. The maintainer will acknowledge a complete report
within five business days and coordinate validation, remediation, and
disclosure. Timelines vary with severity and reproducibility.

## Security boundaries

VectorKit validates persisted checksums and fails closed on corrupt snapshots,
but callers remain responsible for filesystem permissions, embedding-provider
privacy, untrusted input limits, application secrets, and distribution-signing
integrity. Do not report ordinary model-quality differences as security issues.
