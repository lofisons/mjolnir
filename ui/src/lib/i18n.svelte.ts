// Words. English first; other languages are a table from the English words.
// What an instrument calls its parts (bpm, loop, filter, mute, fx) stays in
// English in every language; messages and actions are translated.

const ES: Record<string, string> = {
  // the status line
  "touch to wake the sound": "toca para despertar el sonido",
  "recording · play the pads": "grabando · toca los pads",
  "playing the song": "sonando la canción",
  playing: "sonando",
  stopped: "detenido",
  recording: "grabando",
  "click plays a loop · double click opens it": "un clic toca un loop · doble clic lo abre",
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
  // song
  "add a row": "añadir fila",
  "no loops yet": "aún no hay loops",
  "remove clip": "quitar clip",
  none: "no",
  export: "exportar",
  // export
  close: "cerrar",
  what: "qué",
  as: "como",
  "the song": "la canción",
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
  "the song is empty → place loops on it first, or export the row": "la canción está vacía → coloca loops en ella, o exporta la fila",
  "that file could not be read as sound → try a WAV or an MP3": "ese archivo no se pudo leer como sonido → prueba un WAV o un MP3",
  "the microphone is not available here → use a file": "aquí no hay micrófono → usa un archivo",
  "the recording could not be read": "la grabación no se pudo leer",
  "that file is neither a set nor a sound": "ese archivo no es un set ni un sonido",
  "the demo could not be loaded": "la demo no se pudo cargar",
  "that set could not be copied": "ese set no se pudo copiar",
  "this browser would not keep the set → save it to a file": "este navegador no pudo conservar el set → guárdalo en un archivo",
};

function first(): string {
  try {
    const kept = localStorage.getItem("mjolnir.lang");
    if (kept) return kept;
  } catch {
    /* private window */
  }
  return typeof navigator !== "undefined" && navigator.language?.toLowerCase().startsWith("es") ? "es" : "en";
}

export const lang = $state({ code: first() });

export function setLang(code: string): void {
  lang.code = code;
  document.documentElement.lang = code;
  try {
    localStorage.setItem("mjolnir.lang", code);
  } catch {
    /* private window */
  }
}

/** The words for this language (the English ones when there are no others). */
export function tx(words: string): string {
  return lang.code === "es" ? (ES[words] ?? words) : words;
}

export const KEYS = Object.keys(ES);
