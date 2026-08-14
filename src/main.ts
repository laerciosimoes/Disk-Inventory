import { createApp } from "vue";
import App from "./App.vue";

// A silent blank window (crash before/during mount, or an uncaught error
// afterwards) is worse than an ugly error: there'd be nothing on screen to
// tell the user — or us — what went wrong. This is a last-resort net, not a
// replacement for handling expected errors (network/IO failures) in place.
function showFatalError(message: string): void {
  const pre = document.createElement("pre");
  pre.style.cssText =
    "position:fixed;inset:0;margin:0;padding:1rem;background:#2a0000;color:#ffb4b4;" +
    "font:12px/1.4 ui-monospace,monospace;white-space:pre-wrap;overflow:auto;z-index:99999;";
  pre.textContent = message;
  document.body.appendChild(pre);
}

function describe(err: unknown): string {
  if (err instanceof Error) return `${err.message}\n${err.stack ?? ""}`;
  return String(err);
}

window.addEventListener("error", (event) => {
  showFatalError(`Uncaught error: ${event.message}\n${describe(event.error)}`);
});

window.addEventListener("unhandledrejection", (event) => {
  showFatalError(`Unhandled promise rejection: ${describe(event.reason)}`);
});

const app = createApp(App);
app.config.errorHandler = (err, _instance, info) => {
  showFatalError(`Vue render error (${info}): ${describe(err)}`);
};

try {
  app.mount("#app");
} catch (err) {
  showFatalError(`Failed to mount app: ${describe(err)}`);
}
