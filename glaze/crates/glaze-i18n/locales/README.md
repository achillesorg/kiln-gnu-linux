# Glaze message catalogues

This directory holds Glaze's localisation catalogues. `en.txt` is the **source of
truth**; every other locale falls back to it for any key it does not define.

## File format

Plain UTF-8 text, one message per line:

```text
# a comment — lines whose first non-space character is '#' are ignored
panel.start = Start
```

- The **first** `=` splits key from value, so values may contain `=` and `#`.
- Leading/trailing whitespace around the key and value is trimmed.
- Blank lines and unknown/malformed lines are skipped (a broken line never
  crashes the shell).
- Supported escapes inside a value: `\n` (newline), `\t` (tab), `\r` (carriage
  return), `\\` (backslash). Any other `\x` is kept literally.

Keys must match a constant in `crates/glaze-i18n/src/messages.rs`
(`messages::Keys::ALL`). The test `audit_finds_every_key_in_english` fails if a
key constant has no `en` entry.

## Fallback order

For each key, Glaze resolves in this order:

1. the requested locale's entry,
2. the base-language entry (e.g. `de_AT` → `de`),
3. the `en` entry,
4. **the key itself** — a visible `panel.start` rather than a blank control.

So a partial translation is safe to ship: untranslated keys simply read in
English.

## Adding a locale

1. Copy `en.txt` to `<tag>.txt`, where `<tag>` is a normalised locale:
   lowercase language, optional `_` + uppercase territory — `de`, `pt_BR`,
   `sr_RS`. (The loader normalises `de_DE.UTF-8` → `de_DE` and then walks the
   chain `de_DE` → `de` → `en`.)
2. Translate the values. Only include strings you can translate **correctly**;
   leave a key out rather than guessing — it will fall back to English.
3. Register the file so it is embedded in the binary: add a line to
   `catalog::EMBEDDED` in `src/catalog.rs`:

   ```rust
   ("pt_BR", include_str!("../locales/pt_BR.txt")),
   ```

4. Run `cargo test -p glaze-i18n`. The audit tests check that every key loads and
   is non-empty, and report any locale that is missing keys.
5. Optionally add the language to the docs/status files.

## User / packager overrides

Glaze also loads a catalogue from disk at runtime, which **overrides** the
embedded defaults. Drop a file named `<normalised-tag>.txt` into either
directory (config wins over data):

- `$XDG_CONFIG_HOME/glaze/locales/` (default `~/.config/glaze/locales/`)
- `$XDG_DATA_HOME/glaze/locales/` (default `~/.local/share/glaze/locales/`)

This lets a user fix or extend a translation without rebuilding Glaze. The file
format is identical.
