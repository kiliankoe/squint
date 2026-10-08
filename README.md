# squint

Shows text as large as possible on screen, so you don't have to... *squint*. squint is a macOS take on [sm (screen-message)](https://github.com/nomeata/screen-message).

## Basic Usage

```sh
# Anything passed will be shown directly.
squint Hello world
# Start empty to start typing or pasting what you want.
squint
# A live clock and a countdown.
squint 'It is {clock}'
squint --timer 5m --zero 'Time is up!' 'Back in {countdown}'
# A QR code, here for joining a Wi-Fi network.
squint --qr 'WIFI:T:WPA;S:Cafe Guest;P:latte-art;;'
# "Remote-control" squint just like sm, here showing the uptime.
(while sleep 1; do uptime; printf '\f'; done) | squint -
```

Text is formatted as Markdown: `**bold**`, `*italic*`, `~~strikethrough~~`, `` `code` `` and `#` or `##` headings at the start of a line. Other Markdown, such as lists or links, shows as typed. While you type, squint shows the text unformatted, so the markers stay editable. `--raw` turns formatting off.

`{clock}` shows the current time. `{countdown}` shows the time left when `--timer` or `--until` is set, and `--zero` sets the text shown once it ends. A backslash, as in `\{clock}`, keeps a placeholder as typed. While you type, the available placeholders are listed at the bottom of the screen.

With `--qr`, the text shows as a QR code instead, always dark on light so phones can scan it.

Paste an image with Cmd-V, or pass one with `--image`, to show it as large as possible instead of the text. Typing or Esc removes it again.

Type to edit the text. The cursor appears while typing or clicking and hides after a moment of idling. Esc clears the text (Cmd-Z brings it back), a second Esc quits. Ctrl-I inverts the colors. Ctrl-Q or Cmd-Q quits directly.

With `-`, squint reads stdin. A form feed (`\f`) ends a frame, and each frame replaces the shown text. Input without form feeds is shown once stdin closes.

Run `squint --help` for colors, font, rotation, alignment and padding.

## Differences from sm

- Colors follow the system appearance: black on white in light mode, white on black in dark mode.
- No kiosk mode (`-k`). Markdown replaces Pango markup (`-m`) and is on by default.
