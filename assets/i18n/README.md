# Site translations

The homepage and the board load their interface text from these files. `en.json` is the source and the
fallback: any key missing from another language shows in English.

Current languages: `en`, `zh-CN` (Simplified Chinese), `zh-TW` (Traditional Chinese), `pt-BR`, `es`.
All four non-English languages also ship a translated content overlay (see below), so projects, tools and
news read in the chosen language. Chinese dates and numbers follow the browser's `zh-CN` / `zh-TW` locale,
and `style.css` swaps in CJK font fallbacks for `:lang(zh)`.

- **Add a language:** copy `pt-BR.json`, translate the values (never the keys), save it as `<code>.json`
  (a BCP 47 code such as `fr` or `de`), then add it to `LANGS` and to `pickLang()` in `assets/js/i18n.js`.
- **Placeholders** such as `{n}`, `{ago}` or `{name}` are filled in by the page. Keep them as they are.
- **`_one` / `_other`** keys are the singular and plural forms of a count.
- **`ms.*`, `gw.*` and `stab.*`** translate the milestone, groundwork and stability names. Their English
  lives in `assets/js/scoring.js`, so `en.json` doesn't repeat them.
- **Text only:** translations are inserted as plain text, so HTML in a value shows up literally.

## Translated content

Strings cover the interface. The project summaries, tool notes and news articles come from
`data/tracker.json`, which the scout rewrites every 3 hours, so they are translated separately in
`data/i18n-content/<code>.json`, keyed by id:

```json
{
  "values":  { "approach": {}, "target": {}, "kind": {}, "byline": {}, "evidence": {}, "proof": {} },
  "changes": { "effort": "…{repo}…{n}…", "tool": "…", "repo": "…" },
  "pinned":  { "text": "", "cta": { "title": "", "text": "", "label": "" } },
  "efforts": { "<effort id>": { "summary": "", "highlights": [], "blockers": [], "nukedNote": "" } },
  "tools":   { "<owner/repo>": { "desc": "" } },
  "news":    { "<news id>": { "title": "", "dek": "", "body": [] } }
}
```

`assets/js/app.js` loads the file for the active language and swaps in only the fields it finds, so
anything untranslated — every new effort or story the scout adds — keeps showing in English instead of
going blank. Only the fields above are replaced; scores, stats and repo names come from the tracker.

- **`values`** maps short values that the tracker stores as English: the approach, target and tool-kind
  chips, plus the free-text `byline`, `evidence` and proof-link labels.
- **`changes`** holds templates for the scout's change feed (`Scout spotted … (substance 87).`), filled
  with `{repo}` and `{n}` at render time. A line no template matches keeps the scout's wording, so new
  shapes of change never break the feed.
- **`pinned`** translates the owner's pinned message (`data/pinned.json`), `**bold**` included.
- **Static pages:** `news/<id>.html` and `news/index.html` keep one crawlable English URL each. The same
  overlay is then applied in the browser by `assets/js/page.js`, which also fills in the shared header and
  footer, so a reader who lands there from a search engine gets the same navigation as the homepage and
  the story in their language. Crawlers, which don't run scripts, keep the English text.
- **Not translated:** headlines under *Latest headlines* are quoted from other outlets and stay as written.

Contributions are welcome through a pull request.
