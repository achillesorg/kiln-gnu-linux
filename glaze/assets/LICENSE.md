# Glaze assets — license

All artwork in this directory (`assets/`) is **original work created for the Glaze
project**. It is not derived from, traced from, or packaged with any third-party
icon set, and it contains no Microsoft/Windows artwork, fonts, logos or
trademarks.

## License

The assets are licensed under the **same terms as the Glaze source code**:

```
SPDX-License-Identifier: MIT OR Apache-2.0
Copyright (c) Glaze contributors
```

That is, you may use them under **either** the MIT License **or** the Apache
License, Version 2.0, at your option. The canonical texts of both licenses apply
to these files exactly as they apply to the Glaze codebase; ship them with the
same license files you ship for the code (see the repository-level license
declaration in the root `Cargo.toml`: `license = "MIT OR Apache-2.0"`).

Same `SPDX-License-Identifier` applies to:

| File | Notes |
|---|---|
| `glaze-mark.svg` | the Glaze mark used by the start button |
| `icons/*.svg` | the shell icon set (application, settings, terminal, files, power, network, volume, battery) |

## Provenance

| | |
|---|---|
| Author | Glaze project contributors |
| Origin | authored for Glaze in `assets/` during Phase 0/1 of `GLAZE.md` §8 |
| Method | hand-authored SVG geometry on a 24×24 grid with a 2px optical stroke |
| Prior art used | none. Geometric primitives only (rectangles, circles, arcs, simple paths) |
| Third-party trademarks | none |
| Windows 2000 reference | *structural* inspiration for the desktop layout only — no Windows asset, font, icon or image is used, copied or redrawn |

## Review checklist for new assets

1. Does the artwork contain any element that originates from another project,
   vendor or icon set? If yes, it does not belong here — either recreate it from
   scratch or put it in a clearly labelled third-party directory with its own
   license file, and record it in `docs/architecture.md` / ADR notes.
2. Is the file listed in `assets/README.md` with its purpose?
3. Does it follow the geometry contract (24×24 viewBox, 2px stroke, round caps,
   monochrome-capable)?
4. Is it original to Glaze? Then the `MIT OR Apache-2.0` header above applies.

## Trademark note

"Glaze" and the Glaze mark identify this project. This license grants copyright
rights to the artwork; it does not grant permission to use the Glaze name or mark
to imply endorsement of a modified distribution. Such use is governed by normal
trademark law, not by the copyright license above.
