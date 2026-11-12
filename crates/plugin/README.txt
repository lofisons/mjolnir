Mjolnir · VST 3

The sounds of Mjolnir as an instrument for a DAW: the drum kits, the recorded
instruments, the airs, the leads and the synth sounds, played by the DAW's
notes, with the ready-made drum loops in time with the DAW's song.

Where it goes

  Windows   copy  Windows\Mjolnir.vst3  to  C:\Program Files\Common Files\VST3\
  macOS     copy  macOS/Mjolnir.vst3    to  ~/Library/Audio/Plug-Ins/VST3/
  Linux     copy  Linux/Mjolnir.vst3    to  ~/.vst3/

Then rescan plugins in the DAW. Mjolnir appears under instruments, by lofisons.

macOS: the plugin is signed but not notarised by Apple. If the DAW refuses to
load it, run this once in Terminal:

  xattr -dr com.apple.quarantine ~/Library/Audio/Plug-Ins/VST3/Mjolnir.vst3

Playing it

  Sound     one of 45 sounds. Drum kits play from C1 (note 36) up, one pad per key.
  Beat      off, or a drum loop that a kit plays while the DAW's song plays.
  Filter, Resonance, Drive, Saturation, Reverb, Delay, Volume: the sound's knobs.
  Swing, Tape, Vinyl, Warmth, Room size: the feel and the lo-fi character.

It has no window of its own: the DAW shows these parameters, and all of them
can be automated.

Source and licence (MIT): https://github.com/lofisons/mjolnir
