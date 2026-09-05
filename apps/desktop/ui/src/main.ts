import { createApp } from "vue";
import App from "./App.vue";
import "./style.css";

// Apply the persisted theme before mounting to avoid a dark flash on light.
try {
  if (localStorage.getItem("theme") === "light") {
    document.documentElement.classList.add("light");
  }
} catch {
  /* ignore */
}

createApp(App).mount("#app");
