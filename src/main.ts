import { mount } from "svelte";
import "./styles/theme.css";
import App from "./App.svelte";
import { getLanguage } from "./lib/i18n/index.svelte";

document.documentElement.lang = getLanguage();

export default mount(App, { target: document.getElementById("app")! });
