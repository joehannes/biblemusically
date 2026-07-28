# Translating the UI strings

Two files. **You almost certainly want the small one.**

| File | Strings | What it is |
|---|---|---|
| `english-newly-translatable.json` | **126** | Everything no catalogue has yet. All 15 shipped languages are missing exactly these. |
| `english-complete.json` | 2259 | The whole inventory, if you would rather redo everything from scratch. |

## Why these 126 exist

The extractor capped strings at 140 characters, and the runtime rule refuses anything over 80 that
the inventory does not vouch for. So every explanatory paragraph in the app fell between those two
ceilings and could not be translated into *any* language — including the ones that ship as finished
catalogues. Raising the cap to 400 brought 126 of them into reach; all of them are prose, the
sentences under a setting that explain what it does and what it costs.

## The format

A flat JSON object. **Translate the values. Do not touch the keys** — the key is the English source
text and is what the app matches against at runtime, so a changed key silently never matches.

```json
{
  "Stop idle GPU servers after (minutes, 0 = never)": "GPU-Server nach Leerlauf stoppen (Minuten, 0 = nie)"
}
```

Keep any leading/trailing punctuation and any `—` that starts a fragment: several of these are
sentence continuations rendered next to a value, so the dash is part of the string rather than
decoration.

## Pasting the result back

Send the translated JSON with its language code. It gets merged into the `strings` object of
`src/src/i18n/<code>.json`, which keeps the existing 2133 entries untouched — nothing already
translated is overwritten.

Codes currently shipped: ar de es fr he hi id it ja ko nl pl pt ru zh
