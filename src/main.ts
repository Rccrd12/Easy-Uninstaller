import { mount } from "svelte";
import "./styles/theme.css";
import App from "./App.svelte";
import { getLanguage } from "./lib/i18n/index.svelte";
import { initTheme } from "./lib/theme.svelte";

document.documentElement.lang = getLanguage();
initTheme();

export default mount(App, { target: document.getElementById("app")! });
