import App from "./App.svelte";
import "./styles.css";
import { mount } from "svelte";

function renderFatalError(error: unknown) {
  const target = document.getElementById("app");
  const message = error instanceof Error ? `${error.name}: ${error.message}\n${error.stack ?? ""}` : String(error);

  if (!target) {
    return;
  }

  target.innerHTML = `
    <section class="fatal-error">
      <strong>Torch Overlay failed to start</strong>
      <pre></pre>
    </section>
  `;

  target.querySelector("pre")!.textContent = message;
}

window.addEventListener("error", (event) => {
  renderFatalError(event.error ?? event.message);
});

window.addEventListener("unhandledrejection", (event) => {
  renderFatalError(event.reason);
});

const target = document.getElementById("app");

if (!target) {
  throw new Error("App mount target was not found.");
}

const app = mount(App, { target });

export default app;
