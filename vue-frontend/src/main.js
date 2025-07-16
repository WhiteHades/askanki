import { createApp } from "vue";
import { createPinia } from "pinia";
import "./style.css";
import App from "./App.vue";

function mountVueApp() {
  if (document.getElementById("app")) {
    return;
  }

  const appDiv = document.createElement("div");
  appDiv.id = "app";
  document.body.appendChild(appDiv);

  try {
    const app = createApp(App);
    app.use(createPinia());
    app.mount("#app");
  } catch (error) {
    console.error('Vue: Error during mounting:', error);
  }
}

// always mount directly - onUpdateHook doesn't work in reviewer context
mountVueApp();
