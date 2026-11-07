import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { applyTheme, readThemePreference } from "./theme";
import "pretendard/dist/web/variable/pretendardvariable.css";
import "./styles.css";

applyTheme(readThemePreference());

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
