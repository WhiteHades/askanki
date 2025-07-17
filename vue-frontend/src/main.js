import { createApp } from "vue";
import { createPinia } from "pinia";
import "./style.css";
import App from "./App.vue";

function mountVueApp() {
  if (document.getElementById("ai-panel-root")) {
    return;
  }

  const appDiv = document.createElement("div");
  appDiv.id = "ai-panel-root";
  appDiv.style.position = "fixed";
  appDiv.style.top = "0";
  appDiv.style.left = "0";
  appDiv.style.pointerEvents = "none";
  appDiv.style.zIndex = "999";
  document.body.appendChild(appDiv);

  try {
    const app = createApp(App);
    app.use(createPinia());
    app.mount("#ai-panel-root");
  } catch (error) {
    console.error('Vue: Error during mounting:', error);
  }
}

// always mount directly - onUpdateHook doesn't work in reviewer context
mountVueApp();
