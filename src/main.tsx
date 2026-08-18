import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App";
import { TrackingPipApp } from "./TrackingPipApp";
import { ToolsApp } from "./tools/ToolsApp";
import "./index.css";

const label = getCurrentWindow().label;
const Root =
  label === "tools" ? ToolsApp : label === "tracking-pip" ? TrackingPipApp : App;

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>,
);
