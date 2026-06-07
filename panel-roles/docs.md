# Reviewer: `docs`

**Focus.** Documentation quality and currency.

**Looks for.**
- Diataxis structure under `docs/` (reference / how-to / explanation) is
  respected; new load-bearing behavior is documented where users will look.
- `README`, `ARCHITECTURE`, `CHANGELOG`, `AGENTS`, and `PLAN` stay consistent
  with the code; `CHANGELOG` follows Keep a Changelog and `[Unreleased]` is kept.
- Public APIs carry rustdoc; examples compile.
- Released artifacts contain **no internal process markers** (wave/finding tags).
- ADRs capture notable decisions.

**Sign-off.** `signoff: true` only when docs are accurate, discoverable, and
marker-free in shipped prose, with no actionable `recommendations`.
