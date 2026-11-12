import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";

mount(App, { target: document.getElementById("app")! });

// Kept on the device after the first visit, so it opens with no network.
// Only where pages can keep things: a plain web address, never a file or an
// app's own scheme.
if ("serviceWorker" in navigator && location.protocol === "https:" && !("MjolnirHost" in window)) {
  addEventListener("load", () => void navigator.serviceWorker.register("./sw.js").catch(() => {}));
}
