// The sets a person keeps in this browser: each one a whole project, saved as
// the text of a project file. IndexedDB, because a set with recordings in it
// does not fit in the smaller stores.

export interface SetInfo {
  id: string;
  name: string;
  /** Milliseconds since 1970. */
  updated: number;
  bpm: number;
  tracks: number;
}
interface SetRow extends SetInfo {
  json: string;
}

const DB = "mjolnir";
const STORE = "sets";
let opening: Promise<IDBDatabase> | null = null;

function db(): Promise<IDBDatabase> {
  opening ??= new Promise((ok, err) => {
    const r = indexedDB.open(DB, 1);
    r.onupgradeneeded = () => r.result.createObjectStore(STORE, { keyPath: "id" });
    r.onsuccess = () => ok(r.result);
    r.onerror = () => err(r.error);
  });
  return opening;
}

function run<T>(mode: IDBTransactionMode, work: (s: IDBObjectStore) => IDBRequest<T>): Promise<T> {
  return db().then(
    (d) =>
      new Promise<T>((ok, err) => {
        const r = work(d.transaction(STORE, mode).objectStore(STORE));
        r.onsuccess = () => ok(r.result);
        r.onerror = () => err(r.error);
      }),
  );
}

export const newId = () => `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 8)}`;

/** Every set, the one touched last first. */
export async function listSets(): Promise<SetInfo[]> {
  const rows = await run<SetRow[]>("readonly", (s) => s.getAll()).catch(() => []);
  return rows
    .map(({ id, name, updated, bpm, tracks }) => ({ id, name, updated, bpm, tracks }))
    .sort((a, b) => b.updated - a.updated);
}

export async function loadSet(id: string): Promise<string | null> {
  const row = await run<SetRow | undefined>("readonly", (s) => s.get(id)).catch(() => undefined);
  return row?.json ?? null;
}

export async function saveSet(id: string, json: string): Promise<void> {
  let name = "Untitled";
  let bpm = 0;
  let tracks = 0;
  try {
    const p = JSON.parse(json).project;
    name = p.name || name;
    bpm = Math.round(p.bpm);
    tracks = p.tracks.length;
  } catch {
    /* keep the defaults */
  }
  await run("readwrite", (s) => s.put({ id, name, updated: Date.now(), bpm, tracks, json } satisfies SetRow));
}

export async function deleteSet(id: string): Promise<void> {
  await run("readwrite", (s) => s.delete(id));
}
