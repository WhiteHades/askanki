# AskAnki frontend

The Anki WebView frontend is a React + TypeScript + Vite application. It communicates with Anki through the injected `window.ankiAskAnki` bridge; the bridge is the only supported runtime interface.

## Development

```sh
npm install
npm run dev
npm run typecheck
npm run lint
npm test -- --run
npm run build
```

The production build writes `dist/index.html`, `dist/assets/index.js`, and `dist/assets/index.css` for the add-on loader.
