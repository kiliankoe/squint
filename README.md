# squint

Shows text as large as possible on screen, so you don't have to... *squint*. squint is a macOS take on [sm (screen-message)](https://github.com/nomeata/screen-message).

## Basic Usage

```sh
# Anything passed will be shown directly.
squint Hello world
# Start empty to start typing or pasting what you want.
squint
# "Remote-control" squint just like sm, here showing a live clock.
(while sleep 1; do date +%T; printf '\f'; done) | squint -
```

Type to edit the text. The cursor appears while typing or clicking and hides after a moment of idling. Esc clears the text (Cmd-Z brings it back), a second Esc quits. Ctrl-I inverts the colors. Ctrl-Q or Cmd-Q quits directly.

With `-`, squint reads stdin. A form feed (`\f`) ends a frame, and each frame replaces the shown text. Input without form feeds is shown once stdin closes.

Run `squint --help` for colors, font, rotation, alignment and padding.

## Differences from sm

- Colors follow the system appearance: black on white in light mode, white on black in dark mode.
- No kiosk mode (`-k`) and no Pango markup (`-m`).
