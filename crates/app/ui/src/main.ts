import "pretendard/dist/web/variable/pretendardvariable.css";
import "./app.css";
import { mount } from "svelte";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { setLang } from "./lib/i18n";

// Both windows load this page (W1); the backend names the settings one.
const isSettings = getCurrentWindow().label === "settings";

// The pages load after the language is set: their module-level labels call `t`.
invoke<string>("lang")
  .then(setLang, (e) => console.error("language", e))
  .then(() => (isSettings ? import("./SettingsWindow.svelte") : import("./App.svelte")))
  .then(({ default: page }) => mount(page, { target: document.getElementById("app")! }));
