# Security Policy

## Supported Versions

OxiProto follows a rolling `0.x` release line. Only the **latest published
`0.x` release** of each crate (`oxiproto`, `oxiproto-core`, `oxiproto-build`,
`oxiproto-reflect`, `oxiproto-wkt`, `oxiproto-codegen`, `oxiproto-cli`,
`oxiproto-json`) receives security fixes. Older `0.x` releases are not
patched — please upgrade to the latest version before reporting an issue.

## Reporting a Vulnerability

**Please do not file a public GitHub issue for security vulnerabilities.**
Public issues are visible to everyone before a fix is available, which puts
downstream users at risk.

Instead, report suspected vulnerabilities privately by email to:

**info@kitasan.io**

Please include:

- A description of the vulnerability and its potential impact
- The affected crate(s) and version(s)
- Steps to reproduce (a minimal `.proto` schema and/or byte sequence is ideal
  for wire-format or parser issues)
- Any suggested mitigation, if known

## What to Expect

- Reports are triaged privately by the maintainer.
- We will acknowledge receipt and keep you informed as the issue is
  investigated and fixed.
- Once a fix is released, we will credit the reporter (unless anonymity is
  requested) in the release notes.

## Scope

This policy covers the OxiProto workspace crates themselves — the native
`.proto` parser, the wire-format encoder/decoder, reflection, codegen, and
the CLI. Issues in upstream dependencies (e.g. `prost`, `protox`) should be
reported to those projects directly, though we are happy to help coordinate
if the issue is only reachable through OxiProto.

## Maintainer

COOLJAPAN OU (Team Kitasan)
Security contact: info@kitasan.io
