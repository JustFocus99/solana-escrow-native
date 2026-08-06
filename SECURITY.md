# Security Policy

## Status of this project

This is an unaudited, in-development Solana program. It has not been
deployed to mainnet, has no bug bounty, and carries no guarantee of
correctness. [docs/threat-model.md](docs/threat-model.md) documents the
assumptions its validation logic is designed against; treat anything not
covered there as unreviewed.

None of that lowers the bar for how a report should be handled — a
vulnerability found here now is exactly the kind of thing this project is
trying to catch before any funds are ever at risk.

## Reporting a vulnerability

**Do not open a public GitHub issue, pull request, or discussion for a
suspected vulnerability.** Public disclosure before a fix is available gives
potential attackers a head start against anyone who does end up running this
code.

Instead, report it privately through one of:

- GitHub's private vulnerability reporting for this repository (open the
  **Security** tab → **Report a vulnerability**).
- Email: link.suj@gmail.com — include enough detail to reproduce the issue
  (affected instruction, accounts involved, and the specific check that's
  missing or bypassable).

Please include, where applicable:

- Which instruction (`Initialize`, `Deposit`, `Exchange`, `Cancel`, or
  another) is affected.
- The account configuration or transaction shape that triggers the issue —
  see [docs/threat-model.md](docs/threat-model.md) for the categories of
  attacker-controlled input this program has to defend against (account
  order, duplicate accounts, substituted mints, etc.).
- Whether the issue is a missing validation check, a logic error, or an
  issue in a dependency (e.g. `solana-program`, SPL Token).

## What to expect

This is a solo/small-scale project without a formal SLA. Reports will be
acknowledged and investigated as promptly as possible; a fix or a
mitigation plan will be shared with the reporter before any public
disclosure or changelog entry references the issue.

## Scope

In scope: the program code under `programs/escrow/` and the validation
logic it implements. Out of scope: the Solana runtime itself, the SPL Token
Program, and other upstream dependencies — please report those to their
respective maintainers.
