from aqt import mw, gui_hooks
from aqt.webview import WebContent
from typing import Any

addon_package = mw.addonManager.addonFromModule(__name__)
mw.addonManager.setWebExports(__name__, r"vue-frontend/dist/.*")

def inject_vue_ui(web_content: WebContent, context: Any):
    if not isinstance(context, mw.reviewer.__class__):
        return

    dev = True

    if dev:
        dev_script = '<script type="module" src="http://localhost:5173/src/main.js"></script>'
        web_content.head += dev_script
    else:
        js_path = f"/_addons/{addon_package}/vue-frontend/dist/assets/index.js"
        web_content.js.append(js_path)

def get_ai_config():
    """Get AI assistant config from Anki's config"""
    config = mw.addonManager.getConfig(__name__) or {}
    return {
        'api_key': config.get('api_key', ''),
        'model_name': config.get('model_name', 'gemini-2.5-flash-lite-preview-06-17')
    }

def save_ai_config(api_key: str, model_name: str):
    """Save AI assistant config to Anki's config"""
    config = mw.addonManager.getConfig(__name__) or {}
    config['api_key'] = api_key
    config['model_name'] = model_name
    mw.addonManager.writeConfig(__name__, config)

def webview_bridge():
    """Bridge functions to communicate between webview and Anki"""
    def get_config():
        return get_ai_config()

    def save_config(api_key, model_name):
        save_ai_config(api_key, model_name)

    mw.web.eval(f"""
        window.ankiGetAIConfig = function() {{
            return {get_config()};
        }};
        window.ankiSaveAIConfig = function(apiKey, modelName) {{
            pycmd('save_ai_config:' + apiKey + ':' + modelName);
        }};
    """)

def handle_pycmd(handled, cmd, context):
    """Handle commands from webview"""
    if cmd.startswith('save_ai_config:'):
        parts = cmd.split(':', 2)
        if len(parts) == 3:
            api_key = parts[1]
            model_name = parts[2]
            save_ai_config(api_key, model_name)
        return (True, None)
    return handled

gui_hooks.webview_will_set_content.append(inject_vue_ui)
gui_hooks.webview_did_receive_js_message.append(handle_pycmd)

def on_reviewer_did_show_question(card):
    webview_bridge()

gui_hooks.reviewer_did_show_question.append(on_reviewer_did_show_question)