# Spikes

A spike is a short, time-boxed experiment. It settles a question before the code that depends on it is written. [roadmap.md](roadmap.md) lists every planned spike.

## Rules

- Write PASS and FAIL before running anything.
- Keep to the time box. When it runs out, record what you have and decide.
- A gating spike runs before the work it gates.
- A fallback you take becomes an ADR in `docs/adr/`.
- Keep raw outputs (captures, logs, numbers) next to the entry or in `crates/*/tests/fixtures/`. Redact arguments and anything secret.

## Template

Copy this for each spike and fill it in.

```markdown
## Sx.y Title

- **Milestone / gates:** M?, which work it gates
- **Time box:** ? d
- **Question:** the one question this answers
- **Method:** what was run, on which OS, versions, and hardware
- **PASS if:** …
- **FAIL then:** the fallback
- **Result:** PASS / FAIL (date)
- **Numbers and evidence:** tables, links to fixtures
- **Decision:** what changes in the code or docs (ADR link if a fallback was taken)
```

## Results

*None yet.*
