from aqt import mw, gui_hooks
from aqt.webview import WebContent
from typing import Any

addon_package = mw.addonManager.addonFromModule(__name__)
mw.addonManager.setWebExports(__name__, r"vue-frontend/dist/.*")


def inject_vue_ui(web_content: WebContent, context: Any):
    if not isinstance(context, mw.reviewer.__class__):
        return

    web_content.css.append(
        f"/_addons/{addon_package}/vue-frontend/dist/assets/index.css"
    )
    web_content.js.append(f"/_addons/{addon_package}/vue-frontend/dist/assets/index.js")


gui_hooks.webview_will_set_content.append(inject_vue_ui)
