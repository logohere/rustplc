use dioxus::prelude::*;

use crate::app::AppStore;
use crate::plc_model::PouLanguage;

static MONACO_ASSETS: Asset = asset!("/assets/monaco", AssetOptions::folder());

#[component]
pub fn MonacoEditor(language: PouLanguage) -> Element {
    let language_id = match language {
        PouLanguage::St => "openplc-st",
        PouLanguage::Il => "openplc-il",
        PouLanguage::Python => "python",
        PouLanguage::Cpp => "cpp",
        PouLanguage::Ld | PouLanguage::Fbd => "plaintext",
    };
    rsx! {
        MonacoSurface {
            language_id,
            minimap: true,
            occurrences_highlight: true,
        }
    }
}

#[component]
pub fn ManifestMonaco() -> Element {
    rsx! {
        div { class: "flex h-full w-full flex-col overflow-hidden",
            MonacoSurface {
                language_id: "json",
                minimap: false,
                occurrences_highlight: false,
            }
        }
    }
}

#[component]
fn MonacoSurface(language_id: &'static str, minimap: bool, occurrences_highlight: bool) -> Element {
    let mut store = use_context::<AppStore>();
    let initial =
        serde_json::to_string(&store.current_content()).unwrap_or_else(|_| "\"\"".to_owned());
    let language_json = serde_json::to_string(language_id).expect("language id serializes");
    let vs_base = format!("{MONACO_ASSETS}/vs");
    let vs_json = serde_json::to_string(&vs_base).expect("asset path serializes");
    let minimap_json = if minimap { "true" } else { "false" };
    let occurrences = if occurrences_highlight {
        "'singleFile'"
    } else {
        "'off'"
    };

    use_future(move || {
        let initial = initial.clone();
        let language_json = language_json.clone();
        let vs_json = vs_json.clone();
        async move {
            let script = format!(
                r#"
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
while (!window.require) await sleep(10);
window.require.config({{ paths: {{ vs: {vs_json} }} }});
await new Promise((resolve, reject) => window.require(['vs/editor/editor.main'], resolve, reject));
if (!monaco.languages.getLanguages().some(x => x.id === 'openplc-st')) {{
  monaco.languages.register({{ id: 'openplc-st' }});
  monaco.languages.setMonarchTokensProvider('openplc-st', {{
    ignoreCase: true,
    tokenizer: {{
      root: [
        [/\(\*/, 'comment', '@comment'],
        [/\/\/.*$/, 'comment'],
        [/\b(PROGRAM|END_PROGRAM|FUNCTION|END_FUNCTION|FUNCTION_BLOCK|END_FUNCTION_BLOCK|VAR|VAR_INPUT|VAR_OUTPUT|VAR_IN_OUT|VAR_EXTERNAL|VAR_TEMP|VAR_GLOBAL|END_VAR|IF|THEN|ELSE|ELSIF|END_IF|CASE|OF|END_CASE|FOR|TO|BY|DO|END_FOR|WHILE|END_WHILE|REPEAT|UNTIL|END_REPEAT|RETURN|TRUE|FALSE|AND|OR|XOR|NOT|MOD)\b/, 'keyword'],
        [/\b(BOOL|BYTE|WORD|DWORD|LWORD|SINT|INT|DINT|LINT|USINT|UINT|UDINT|ULINT|REAL|LREAL|TIME|DATE|TOD|DT|STRING|WSTRING)\b/, 'type'],
        [/%[IQM][XWDLB]?[0-9.]+/, 'number'],
        [/\b[0-9]+(?:\.[0-9]+)?\b/, 'number'],
        [/'[^']*'|\"[^\"]*\"/, 'string'],
        [/[A-Za-z_][\w]*/, 'identifier']
      ],
      comment: [[/[^*(]+/, 'comment'], [/\*\)/, 'comment', '@pop'], [/[*(]/, 'comment']]
    }}
  }});
}}
if (!monaco.languages.getLanguages().some(x => x.id === 'openplc-il')) {{
  monaco.languages.register({{ id: 'openplc-il' }});
}}
const host = document.getElementById('openplc-monaco');
if (!host) return;
if (window.__openplcMonaco) {{
  try {{ window.__openplcMonaco.dispose(); }} catch (_) {{}}
}}
const editor = monaco.editor.create(host, {{
  value: {initial},
  language: {language_json},
  automaticLayout: true,
  minimap: {{ enabled: {minimap_json} }},
  fontSize: 13,
  tabSize: 2,
  insertSpaces: true,
  formatOnPaste: true,
  formatOnType: true,
  renderWhitespace: 'selection',
  scrollBeyondLastLine: false,
  occurrencesHighlight: {occurrences},
}});
window.__openplcMonaco = editor;
editor.onDidChangeModelContent(() => {{
  dioxus.send({{ type: 'change', value: editor.getValue() }});
}});
for (;;) {{
  const msg = await dioxus.recv();
  if (!msg) continue;
  if (msg.type === 'setValue' && editor.getValue() !== msg.value) {{
    editor.setValue(msg.value ?? '');
  }} else if (msg.type === 'focus') {{
    editor.focus();
  }} else if (msg.type === 'dispose') {{
    editor.dispose();
    if (window.__openplcMonaco === editor) window.__openplcMonaco = null;
    return;
  }}
}}
"#
            );
            let mut eval = document::eval(&script);
            while let Ok(message) = eval.recv::<serde_json::Value>().await {
                if message.get("type").and_then(|value| value.as_str()) == Some("change") {
                    if let Some(value) = message.get("value").and_then(|value| value.as_str()) {
                        store.set_current_content(value.to_owned());
                    }
                }
            }
        }
    });

    rsx! {
        document::Stylesheet { href: format!("{MONACO_ASSETS}/vs/editor/editor.main.css") }
        document::Script { src: format!("{MONACO_ASSETS}/vs/loader.js") }
        div { id: "openplc-monaco", class: "h-full w-full overflow-hidden rounded-md" }
    }
}
