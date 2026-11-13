// Words. English first; other languages are a table from the English words.
// What an instrument calls its parts (bpm, loop, filter, mute, fx) stays in
// English in every language; messages and actions are translated.

const ES: Record<string, string> = {
  // the status line
  "touch to wake the sound": "toca para despertar el sonido",
  "recording · play the pads": "grabando · toca los pads",
  "playing the timeline": "sonando el timeline",
  playing: "sonando",
  stopped: "detenido",
  recording: "grabando",
  "click: play · double: open · hold: more": "clic: tocar · doble: abrir · mantener: más",
  "touch + to begin": "toca + para empezar",
  "count in": "cuenta",
  rendering: "renderizando",
  "loading the engine": "cargando el motor",
  undo: "deshacer",
  redo: "rehacer",
  // loops
  section: "sección",
  new: "nueva",
  copy: "copiar",
  delete: "borrar",
  "new track": "pista nueva",
  drums: "batería",
  keys: "teclas",
  // inside a track
  capture: "capturar",
  notes: "notas",
  sound: "sonido",
  "showing bar": "viendo compás",
  follow: "seguir",
  note: "nota",
  chords: "acordes",
  off: "no",
  edit: "editar",
  level: "nivel",
  chance: "azar",
  repeat: "repetir",
  "play the row": "tocar la fila",
  "copy to the next empty row": "copiar a la siguiente fila vacía",
  "copy to the next free row": "copiar a la siguiente fila libre",
  "clear the row": "vaciar la fila",
  "there is no empty section left to copy into": "no queda una sección vacía donde copiar",
  open: "abrir",
  "delete loop": "borrar loop",
  unmute: "quitar mute",
  move: "transportar",
  harmony: "armonizar",
  octave: "octava",
  above: "arriba",
  below: "abajo",
  "add a": "añadir",
  "3rd": "3ra",
  "4th": "4ta",
  "5th": "5ta",
  "6th": "6ta",
  oct: "8va",
  copied: "copiado",
  paste: "pegar",
  snap: "ajustar",
  clear: "vaciar",
  "forget knob moves": "olvidar movimientos",
  remove: "quitar",
  // sound
  yours: "tuyos",
  "from the tracks": "de las pistas",
  "synth kits": "kits sintéticos",
  "play it": "tocarlo",
  "across the keys": "en el teclado",
  "as grains": "en granos",
  "sliced on the pads": "troceado en los pads",
  "from a file": "desde un archivo",
  "record the microphone": "grabar el micrófono",
  "stop and keep": "parar y conservar",
  "up to twenty seconds; it replaces this track's sound": "hasta veinte segundos; reemplaza el sonido de esta pista",
  "remove track": "quitar pista",
  "open the sound": "abrir el sonido",
  // timeline
  "add a row": "añadir fila",
  "no loops yet": "aún no hay loops",
  "remove clip": "quitar clip",
  none: "no",
  export: "exportar",
  // export
  close: "cerrar",
  what: "qué",
  as: "como",
  "the timeline": "el timeline",
  "the mix": "la mezcla",
  "one file per track": "un archivo por pista",
  row: "fila",
  "as a loop": "como loop",
  // set
  sets: "sets",
  "new set": "set nuevo",
  "open a file": "abrir un archivo",
  "save this set to a file": "guardar este set en un archivo",
  "listen to a demo": "escucha una demo",
  "this set": "este set",
  metronome: "metrónomo",
  "snap to the grid": "ajustar a la rejilla",
  "this device": "este dispositivo",
  dark: "oscuro",
  paper: "papel",
  "how to play": "cómo se toca",
  output: "salida",
  "no controller": "sin controlador",
  "remove for good": "quitar para siempre",
  keep: "conservar",
  language: "idioma",
  now: "ahora",
  // things that go wrong
  "copy a loop first": "copia un loop primero",
  "choose something to export": "elige qué exportar",
  "this row has no loops": "esta fila no tiene loops",
  "there was nothing to render": "no había nada que renderizar",
  "the timeline is empty → place loops on it first, or export the row": "el timeline está vacío → coloca loops en él, o exporta la fila",
  "that file could not be read as sound → try a WAV or an MP3": "ese archivo no se pudo leer como sonido → prueba un WAV o un MP3",
  "the microphone is not available here → use a file": "aquí no hay micrófono → usa un archivo",
  "the recording could not be read": "la grabación no se pudo leer",
  "that file is neither a set nor a sound": "ese archivo no es un set ni un sonido",
  "the demo could not be loaded": "la demo no se pudo cargar",
  "that set could not be copied": "ese set no se pudo copiar",
  "this browser would not keep the set → save it to a file": "este navegador no pudo conservar el set → guárdalo en un archivo",
};

// English until a person chooses another language in `set`. The instrument's
// own words stay English in every language, so following the device's
// language on its own left a screen that read as English with stray words.
function first(): string {
  try {
    const kept = localStorage.getItem("mjolnir.lang.chosen");
    if (kept) return kept;
  } catch {
    /* private window */
  }
  return "en";
}

export const lang = $state({ code: first() });

export function setLang(code: string): void {
  lang.code = code;
  document.documentElement.lang = code;
  try {
    localStorage.setItem("mjolnir.lang.chosen", code);
  } catch {
    /* private window */
  }
}

/** The words for this language (the English ones when there are no others). */
export function tx(words: string): string {
  return lang.code === "es" ? (ES[words] ?? words) : words;
}

export const KEYS = Object.keys(ES);
