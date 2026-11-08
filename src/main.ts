import { mount } from "svelte";
import "./styles/theme.css";
import App from "./App.svelte";

export default mount(App, { target: document.getElementById("app")! });
