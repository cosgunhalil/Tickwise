# Security Policy

## Supported versions

Tickwise is in early development and has no published release yet. Until 1.0, only the latest state of the `main` branch receives security fixes.

| Version | Supported |
|---|---|
| latest `main` | ✓ |
| anything else | ✗ |

## What counts as a security issue

Tickwise parses `.rec` and `.dump` files that may come from untrusted sources, for example a recording a player attaches to a bug report. Anything that breaks the safety of that parsing is a security issue:

- Memory unsafety, crashes, or panics triggered by a crafted `.rec` or `.dump` file
- Unbounded memory or CPU consumption from a small crafted input
- Any way a recording file can cause behavior outside of reading and reporting

Ordinary bugs, wrong diff output, or desyncs in your own simulation are not security issues. Please use the regular issue templates for those.

## How to report

**Do not open a public issue for a vulnerability.**

Report it privately through GitHub's private vulnerability reporting at
[https://github.com/cosgunhalil/Tickwise/security/advisories/new](https://github.com/cosgunhalil/Tickwise/security/advisories/new).

Include what you can: the affected code path, a crafted input file if you have one, and the impact you believe it has. A minimal reproduction is worth more than a long description.

## What to expect

1. You get an acknowledgment within 7 days.
2. We investigate and keep you updated in the advisory thread.
3. Once a fix lands, the advisory is published and you are credited, unless you prefer otherwise.

This is a solo-maintained project in its early days, so response times are best effort. The 7 day acknowledgment is a commitment; everything faster is a good day.
