# Changelog

## Unreleased

- An agent can make music in Mjolnir through the Model Context Protocol: write loops, build drum parts, shape synths and kits, arrange a song. In the desktop app (**Agent → Let an agent play**), on the set that is open; or as `mjolnir-mcp`, with no window, on a set as a file, with WAV and MIDI out. See [guide/AGENTS.md](guide/AGENTS.md).
- A synth sound can be renamed.

## 0.4.0

The same instrument with less on screen: every option is still there, one touch further in.

- A first visit asks one thing: **start a beat** or **listen first**.
- Loops shows the rows in use and one to grow into; empty cells stay quiet until the pointer or a finger is near. A section's tools moved into its letter's menu, with **rename**.
- Drums are lanes of steps, one per sound, with the pads under them and a fold for the pads. **edit** holds beats to start from and hats to lay over them.
- Inside a track there is one row at the top: back, the tracks, the section, and **sound**.
- The key and the tempo are changed on the screen, from any face.
- Timeline: one section in hand instead of a picker on every lane; fades, zoom and clear behind **edit**.
- Mix on a phone: each strip shows its level and gates; `···` opens its pan, room and echo.
- A new melodic loop starts two bars long. A new set opens on the loops.
- Escape closes the sheet or menu that is open and leaves the track where it is.

## 0.3.0

Less in the way between an idea and a loop.

- The notes view is rebuilt around the piano roll: one line of tools (note length, bars, chords, edit) that open as sheets over the roll, a keyboard that folds away, taller rows and bigger grips on touch, and a picked note's detail in the bar, so nothing jumps.
- Hold a loop, or right click it: open, copy, paste, copy to the next free row, delete. On a track's name: open, mute, remove. On a section's letter: play the row, copy it to the next empty row, clear it.
- `+` adds a track from a menu where the hand is, and the new track opens on its sounds. New tracks start with a sound that is on the shelves (the Shadow kit, the soft piano).
- The tempo is changed on the screen: touch the bpm for −, +, and tap.
- Drums: a step's detail opens when the step is held, in the header's own line, so the pads never move. Writing a step no longer opens it, and the right button no longer switches a step off.
- A phone gives the screen less height and the work more.
- The song face is called **timeline**.
- English until another language is chosen in `set`: following the device's language left stray translated words on an English screen.

## 0.2.0

- A VST 3 instrument for Windows, macOS and Linux: the 45 sounds, the seven knobs, tape, vinyl and warmth, and the ready-made drum loops in time with the host's song.
- A public site (`site/`, built into `docs/`): a front page with the instrument playing inside it, and the web app under `app/`.

- The piano roll writes a note only on a tap: a finger that scrolls the sheet, up, down or sideways, leaves it untouched.
- A loop told to stop shows it (a red light, dimmed notes) while it plays out its bar; a second touch stops it at once instead of starting it again.
- **move** carries every note of a loop to another octave or another note of the key.
- **harmony** doubles a loop at a third, fourth, fifth, sixth or octave of the key, above or below.

## 0.1.0

The first Mjolnir.

- Loops, drum steps, a piano roll, chords, capture and recording.
- Sounds from lofisons' own tracks (two kits, two chop kits, subs, airs, leads), seven real instruments, a ten-engine macro synth with 24 sounds, three synth kits, and your own samples.
- One plate reverb and one tape echo for the whole mix; pump; tape, vinyl and warmth; eight punch effects.
- A mixer, a lane timeline, export of mix, stems, MIDI and single rows.
- A library of sets, three demos, files you own.
- English and Spanish; dark and paper; MIDI in.
- A web app that installs and works offline; desktop apps for Linux, Windows and macOS; an Android app.
