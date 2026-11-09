// The Boost editor's code, like Arc's: Monaco with a CSS and a JavaScript model, the JavaScript
// knowing the DOM's types and the page's own globals. The panel (app/macos/BoostEditor.swift)
// sets the code, shows a model, inserts picked selectors, and hears each change.
const post = (message) => window.webkit?.messageHandlers.boost.postMessage(message);

require.config({ paths: { vs: "vs" } });
require(["vs/editor/editor.main"], () => {
  const typescript = monaco.typescript ?? monaco.languages.typescript;
  typescript.javascriptDefaults.setCompilerOptions({
    target: typescript.ScriptTarget.ES2022,
    allowNonTsExtensions: true,
    lib: ["es2022", "dom", "dom.iterable"],
  });
  const models = {
    css: monaco.editor.createModel("", "css", monaco.Uri.parse("file:///boost.css")),
    js: monaco.editor.createModel("", "javascript", monaco.Uri.parse("file:///boost.js")),
  };
  const editor = monaco.editor.create(document.getElementById("editor"), {
    model: models.css,
    theme: "vs-dark",
    fontSize: 12,
    minimap: { enabled: false },
    scrollBeyondLastLine: false,
    automaticLayout: true,
    fixedOverflowWidgets: true,
    tabSize: 2,
  });
  // Setting the code from the app is not a change of the user's.
  let setting = false;
  for (const [kind, model] of Object.entries(models)) {
    model.onDidChangeContent(() => setting || post({ kind, text: model.getValue() }));
  }
  let globals;
  window.boost = {
    set(kind, text) {
      const model = models[kind];
      if (model.getValue() === text) return;
      setting = true;
      model.setValue(text);
      setting = false;
    },
    show(kind) {
      editor.setModel(models[kind]);
      editor.focus();
    },
    insert(text) {
      editor.executeEdits("arbalete", [{ range: editor.getSelection(), text, forceMoveMarkers: true }]);
      editor.focus();
    },
    // The page's own globals, which the DOM's types do not know.
    globals(names) {
      globals?.dispose();
      const declarations = names.map((name) => `declare var ${name}: any;`).join("\n");
      globals = typescript.javascriptDefaults.addExtraLib(declarations, "file:///page-globals.d.ts");
    },
  };
  post({ ready: true });
});
