// Stub for monaco-editor in vitest unit (jsdom) environments.
// Monaco requires a real DOM worker + blob URL infrastructure unavailable in jsdom.
// CodeEditor.svelte dynamically imports this; the stub prevents resolution failures
// during module import tests without breaking browser-environment tests.

export const editor = {
  create: () => ({
    dispose: () => {},
    setValue: () => {},
    getValue: () => '',
    getModel: () => null,
    updateOptions: () => {},
    layout: () => {},
    onDidChangeModelContent: () => ({ dispose: () => {} }),
  }),
  createModel: () => null,
  setModelLanguage: () => {},
  defineTheme: () => {},
  setTheme: () => {},
};

export const languages = {
  register: () => {},
  setMonarchTokensProvider: () => {},
  registerCompletionItemProvider: () => ({ dispose: () => {} }),
  CompletionItemKind: {},
};

export const KeyMod = { CtrlCmd: 0, Shift: 0 };
export const KeyCode = { KeyS: 0 };

export default { editor, languages, KeyMod, KeyCode };
