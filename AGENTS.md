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

Use `egui_icons` as the first choice for new or changed UI icons. Use another source only when no suitable icon exists, and document the reason. This applies to custom UI icons; retain the app brand assets, native macOS window controls, and egui built-in widget drawing.
