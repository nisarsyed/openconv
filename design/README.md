# design

The agreed design for the macOS app, kept with the code. It's exported from
the design canvas after each round is signed off.

- **`tokens.json`** is the one place token values live: type, space, radii,
  bar and panel sizes, colour in light and dark, the four accents, avatar
  pairs, and what each Appearance setting changes. It uses the
  [W3C Design Tokens](https://www.designtokens.org/tr/drafts/format/) format.
  Colours with alpha are `#RRGGBBAA`. The SwiftUI theme is generated from it
  (#130); don't hand-edit the generated Swift.
- **`frames/`** holds PNG snapshots of every agreed frame, in light and dark,
  at 1×. Use them to build and review against. The Settings › Appearance
  frame shows every user setting.
- **`scripts/`** has the export:
  - `make-tokens.py` reads the canvas stylesheet and writes `tokens.json`.
  - `export-frames.py` renders each frame with headless Chrome, using
    `render-board.mjs` to fill in a board's template.

What was decided, and why, is in [`planning/design.md`](../planning/design.md):
the structure, the decisions from each round, and every state and its
wording.

## The canvas

https://claude.ai/artifact/8dJr9w1ZyUTyV9bMAZV5qQ is the workspace. It's
private to its owner, so ask them for access. It has every frame, plus the
option sheets that record what was compared. New design work, such as the
#131 addendum, happens there first. These files only change after a round is
signed off.

## Updating after a signed-off round

1. Download the canvas project. In a Claude session, the Artifact tool's
   `read` with `paths` saves it under `project/`.
2. Regenerate both outputs:

   ```sh
   python3 design/scripts/make-tokens.py <canvas>/project/r2/oc.css design/tokens.json
   python3 design/scripts/export-frames.py <canvas>/project/r2 design/frames
   ```

   `export-frames.py` needs Google Chrome and Node. Its frame list is at the
   top of the script.
3. Update `planning/design.md` with the decisions, and open a PR.
