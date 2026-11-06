## Git commits

Use the `git-commit` skill for every commit. Conventional Commits with an English `type` and `scope`, and a Chinese `description`:

```
<type>(<scope>): 中文描述
```

## Agent skills

### Issue tracker

Issues and specs are markdown files under `.scratch/<feature>/`. See `docs/agents/issue-tracker.md`.

### Triage labels

Five canonical roles map 1:1 to their own names; state lives in a `Status:` line in each issue file. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: `GLOSSARY.md` + `docs/adr/` at the repo root. See `docs/agents/domain.md`.

## UI icons

Use GPUI Kit's bundled icons for the GPUI frontend, and `egui_icons` for the egui frontend retained during migration. Use another source only when the frontend's icon family has no suitable icon, and document the reason. Retain the app brand assets, native window controls, and framework built-in widget drawing.

## Root README scope

Keep root-level README files (including `README.md` and `README.zh-CN.md`) focused on the product overview and general usage. Do not add explanations dedicated to individual project types; put those details under `docs/` and link to them when needed.
