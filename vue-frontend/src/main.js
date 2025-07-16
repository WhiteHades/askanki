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

  const app = createApp(App);
  app.use(createPinia());
  app.mount("#app");
}

if (window.onUpdateHook) {
  window.onUpdateHook.push(mountVueApp);
} else {
  mountVueApp();
}
