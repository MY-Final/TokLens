import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

type AppInfo = {
  name: string;
  version: string;
};

function App() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<AppInfo>("app_info")
      .then(setInfo)
      .catch((err: unknown) => setError(String(err)));
  }, []);

  const status = error
    ? `Rust backend not connected (likely running in a plain browser): ${error}`
    : info
      ? `Rust backend connected - ${info.name} v${info.version}`
      : "Checking Rust backend connection...";

  return (
    <main className="container">
      <h1 className="title">TokLens</h1>
      <p className="stack">Tauri 2 / React / TypeScript / Vite / Rust</p>
      <p className="status">{status}</p>
    </main>
  );
}

export default App;