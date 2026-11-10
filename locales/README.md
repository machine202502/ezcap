# Interface translations

`ezcap` reads its window and tray texts from these JSON catalogs. Nothing is
compiled into the executable, so a language can be fixed or added by editing a
file.

- `ru.json` — Русский
- `en.json` — English (also the fallback; the keys are the English source strings)
- `zh.json` — 中文

English is also compiled into `ezcap.exe`, so the portable zip ships no locale
files at all and works in English on its own. The installer puts this folder next
to `ezcap.exe`. A catalog in `%LOCALAPPDATA%\ezcap\locales` overrides the
shipped one with the same code, so a translation can be replaced without touching
Program Files; the same works for a portable copy, where the files are simply
placed next to the executable.

## Format

```json
{
  "code": "ru",
  "name": "Русский",
  "strings": {
    "Settings": "Настройки",
    "{fps} FPS": "{fps} FPS"
  }
}
```

- `code` is the file's language code and what **Settings → Language** saves.
- `name` is the native name shown in that list.
- `strings` maps an English source string to its translation. A missing key
  falls back to the English text, so a partial catalog is safe.
- `{placeholders}` must be kept (they are substituted at run time); their order
  and position may change.
- Keys with several spaces, such as `"Stop capture  {time}"`, keep them: the
  spacing is part of the rendered layout.

## Applying a change

The language is chosen in **Settings → Language** ("System" follows the Windows
display language). Labels are created when the window opens, so the choice
applies on the next start; the dialog says so. Adding a catalog file makes it
appear in the list immediately.
