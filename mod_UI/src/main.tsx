import ReactDOM from "react-dom/client";
import App from "./App";

window.addEventListener("unhandledrejection", (event) => {
  const reason = event.reason;
  const message =
    reason instanceof Error
      ? `${reason.name}: ${reason.message}`
      : typeof reason === "string"
        ? reason
        : (() => {
            try {
              return JSON.stringify(reason);
            } catch {
              return String(reason);
            }
          })();
  console.error("[unhandledrejection]", message, reason instanceof Error ? reason.stack : undefined);
});

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <App />,
);
