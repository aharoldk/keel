import { useMemo, useRef } from "react";
import CodeMirror from "@uiw/react-codemirror";
import {
  Decoration,
  type DecorationSet,
  EditorView,
  keymap,
  placeholder as cmPlaceholder,
  ViewPlugin,
  type ViewUpdate,
} from "@codemirror/view";
import type { Range } from "@codemirror/state";
import { redo, redoDepth, selectAll, undo, undoDepth } from "@codemirror/commands";
import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";
import type { Extension } from "@codemirror/state";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import {
  autocompletion,
  type Completion,
  type CompletionContext,
  type CompletionResult,
} from "@codemirror/autocomplete";
import { tags as t } from "@lezer/highlight";
import { json } from "@codemirror/lang-json";
import { yaml } from "@codemirror/lang-yaml";
import { javascript } from "@codemirror/lang-javascript";
import { linter, type Diagnostic } from "@codemirror/lint";
import { searchKeymap, highlightSelectionMatches } from "@codemirror/search";
import { useKeel } from "@/state/store";
import { useEditContextMenu } from "@/components/EditContextMenu";
import type { VariableSuggestion } from "./variables";

export type CodeLanguage = "json" | "yaml" | "javascript" | "text";

interface CodeEditorProps {
  value: string;
  onChange?: (v: string) => void;
  language?: CodeLanguage;
  readOnly?: boolean;
  height?: string;
  lineNumbers?: boolean;
  placeholder?: string;
  /**
   * "app" (default) renders the boxed input-style container; "editor"
   * renders a borderless native code area (compact line numbers, no
   * outline) that follows the app theme — white in light, dark in dark.
   */
  appearance?: "app" | "editor";
  /** Names offered for `{{…}}` completion (env/secret/collection/folder). */
  variables?: VariableSuggestion[];
}

const jsonLinter = linter((view) => {
  const text = view.state.doc.toString();
  const diags: Diagnostic[] = [];
  if (!text.trim()) return diags;
  try {
    // Tags inside JSON strings (`"password": "#{body.x}"`) parse as-is.
    JSON.parse(text);
  } catch (e) {
    try {
      // A bare `#{…}` tag outside a string is mid-edit: mask it with a
      // same-length quoted placeholder so error positions stay accurate.
      JSON.parse(
        text.replace(/#\{[^{}#]*\}/g, (m) => `"${".".repeat(Math.max(0, m.length - 2))}"`),
      );
      return diags;
    } catch {
      const msg = e instanceof Error ? e.message : String(e);
      const m = /position (\d+)/i.exec(msg);
      let pos = m ? Number(m[1]) : 0;
      if (!Number.isFinite(pos) || pos < 0) pos = 0;
      pos = Math.min(pos, Math.max(0, text.length - 1));
      diags.push({
        from: pos,
        to: Math.min(pos + 1, text.length),
        severity: "error",
        message: msg,
      });
    }
  }
  return diags;
});

// Syntax colors reference the theme CSS variables, which flip with
// [data-theme] — so one style stays readable in both dark and light.
// Without this, basicSetup's fallback defaultHighlightStyle paints
// light-background colors (e.g. #a11) that are unreadable on dark.
const syntaxColors = HighlightStyle.define([
  { tag: t.propertyName, color: "var(--method-get)" },
  { tag: t.string, color: "var(--method-post)" },
  { tag: t.number, color: "var(--method-put)" },
  { tag: [t.bool, t.null, t.atom, t.keyword], color: "var(--method-patch)" },
  { tag: t.comment, color: "var(--fg-2)", fontStyle: "italic" },
  { tag: t.punctuation, color: "var(--fg-2)" },
]);

// Fixed muted light palette for the white editor surface — deliberately
// independent of [data-theme] so it stays correct on white even when the
// app itself is dark.
const lightSyntaxColors = HighlightStyle.define([
  { tag: [t.keyword, t.definitionKeyword, t.modifier], color: "#8250df" },
  { tag: [t.bool, t.null, t.atom], color: "#8250df" },
  { tag: t.string, color: "#1a7f37" },
  { tag: t.number, color: "#953800" },
  { tag: t.function(t.variableName), color: "#0550ae" },
  { tag: t.function(t.propertyName), color: "#0550ae" },
  { tag: t.propertyName, color: "#0550ae" },
  { tag: t.variableName, color: "#24292f" },
  { tag: t.comment, color: "#8b949e", fontStyle: "italic" },
  { tag: t.punctuation, color: "#57606a" },
  { tag: t.operator, color: "#57606a" },
]);

const SOURCE_LABEL: Record<VariableSuggestion["source"], string> = {
  env: "env",
  secret: "secret",
  collection: "collection",
  folder: "folder",
  prev: "prev",
};

/** Colors `#{…}` previous-response tags with the patch accent. */
function highlightPrevTags(): ViewPlugin<{ decorations: DecorationSet }> {
  const make = (text: string) => {
    const ranges: Range<Decoration>[] = [];
    const re = /#\{([^{}#]*)\}/g;
    let m: RegExpExecArray | null;
    while ((m = re.exec(text))) {
      ranges.push(
        Decoration.mark({ attributes: { style: "color: var(--method-patch)" } }).range(
          m.index,
          m.index + m[0].length,
        ),
      );
    }
    return Decoration.set(ranges);
  };
  return ViewPlugin.fromClass(
    class {
      decorations: DecorationSet;
      constructor(view: EditorView) {
        this.decorations = make(view.state.doc.toString());
      }
      update(update: ViewUpdate) {
        if (update.docChanged) this.decorations = make(update.state.doc.toString());
      }
    },
    { decorations: (v) => v.decorations },
  );
}

/** `{{` triggers variable completion, `#{` triggers previous-response refs. */
function variableCompletion(variables: VariableSuggestion[] | undefined): Extension | null {
  if (!variables || variables.length === 0) return null;
  const braces = variables.filter((v) => v.source !== "prev");
  const prevs = variables.filter((v) => v.source === "prev");
  if (braces.length === 0 && prevs.length === 0) return null;
  const braceOptions: Completion[] = braces.map((v) => ({
    label: v.name,
    detail: [SOURCE_LABEL[v.source], v.value].filter((s) => s).join(" · "),
    apply: `${v.name}}}`,
    type: v.source === "secret" ? "keyword" : "variable",
    boost: v.source === "env" ? 10 : v.source === "folder" ? 5 : 0,
  }));
  const prevOptions: Completion[] = prevs.map((v) => ({
    label: v.name,
    detail: "previous response",
    apply: `${v.name}}}`,
    type: "property",
  }));
  return autocompletion({
    icons: false,
    override: [
      (ctx: CompletionContext): CompletionResult | null => {
        const before = ctx.matchBefore(/\{\{[^{}]*/);
        if (!before || braceOptions.length === 0) return null;
        return { from: before.from + 2, options: braceOptions, validFor: /^[^{}]*$/ };
      },
      (ctx: CompletionContext): CompletionResult | null => {
        const before = ctx.matchBefore(/#\{[^{}#]*/);
        if (!before || prevOptions.length === 0) return null;
        return { from: before.from + 2, options: prevOptions, validFor: /^[^{}#]*$/ };
      },
    ],
  });
}

function makeTheme(dark: boolean, fontSize: number): Extension {
  return EditorView.theme(
    {
      "&": {
        backgroundColor: "transparent",
        colorScheme: dark ? "dark" : "light",
        fontSize: `${fontSize}px`,
      },
      ".cm-content": {
        fontFamily: "var(--mono)",
        caretColor: "var(--fg-0)",
      },
      ".cm-scroller": {
        fontFamily: "var(--mono)",
        lineHeight: "1.5",
      },
      ".cm-gutters": {
        backgroundColor: "transparent",
        border: "none",
        color: "var(--fg-2)",
      },
      ".cm-activeLine": { backgroundColor: "transparent" },
      ".cm-activeLineGutter": { backgroundColor: "transparent" },
      "&.cm-focused": { outline: "none" },
      "&.cm-focused .cm-selectionBackground, .cm-content ::selection": {
        backgroundColor: "var(--sel)",
      },
      ".cm-cursor, .cm-dropCursor": { borderLeftColor: "var(--fg-0)" },
      ".cm-selectionBackground": { backgroundColor: "var(--sel)" },
      ".cm-searchMatch": {
        backgroundColor: "var(--accent-soft)",
        outline: "1px solid var(--accent)",
      },
      ".cm-searchMatch-selected": {
        backgroundColor: "var(--accent-soft)",
        outline: "1px solid var(--accent-strong)",
      },
      ".cm-tooltip": {
        backgroundColor: "var(--bg-3)",
        border: "1px solid var(--line-1)",
        fontFamily: "var(--mono)",
        fontSize: "11px",
        color: "var(--fg-0)",
        overflow: "hidden",
      },
      ".cm-tooltip.cm-tooltip-autocomplete > ul": {
        fontFamily: "var(--mono)",
        maxHeight: "180px",
      },
      ".cm-tooltip-autocomplete ul li": {
        color: "var(--fg-1)",
      },
      ".cm-tooltip-autocomplete ul li[aria-selected]": {
        backgroundColor: "var(--accent-soft)",
        color: "var(--fg-0)",
      },
      ".cm-completionDetail": {
        color: "var(--fg-2)",
        fontStyle: "normal",
      },
      ".cm-panels": {
        backgroundColor: "var(--bg-2)",
        color: "var(--fg-0)",
        borderBottom: "1px solid var(--line-0)",
        borderTop: "1px solid var(--line-0)",
      },
      ".cm-panel input, .cm-panel button": {
        backgroundColor: "var(--bg-2)",
        border: "1px solid var(--line-1)",
        color: "var(--fg-0)",
        fontSize: "11px",
        borderRadius: "4px",
      },
      ".cm-foldPlaceholder": {
        backgroundColor: "var(--bg-3)",
        border: "none",
        color: "var(--fg-1)",
      },
      ".cm-diagnostic-error": { color: "var(--danger)" },
      ".cm-placeholder": { color: "var(--fg-2)", fontStyle: "normal" },
    },
    { dark },
  );
}

// Palettes for the borderless "editor" appearance. The light palette is
// fixed (white surface, muted GitHub-light-like colors); the dark palette
// references the theme CSS variables so it flips with [data-theme].
interface PlainPalette {
  fg: string;
  gutter: string;
  sel: string;
  searchBg: string;
  searchOutline: string;
  searchSelectedOutline: string;
  tooltipBg: string;
  tooltipBorder: string;
  tooltipFg: string;
  tooltipItem: string;
  tooltipSelBg: string;
  tooltipSelFg: string;
  completionDetail: string;
  panelBg: string;
  panelBorder: string;
  panelInputBg: string;
  panelInputBorder: string;
  foldBg: string;
  foldFg: string;
  danger: string;
  placeholder: string;
}

const PLAIN_LIGHT: PlainPalette = {
  fg: "#24292f",
  gutter: "#9aa4b2",
  sel: "rgba(9, 105, 218, 0.12)",
  searchBg: "rgba(9, 105, 218, 0.12)",
  searchOutline: "rgba(9, 105, 218, 0.45)",
  searchSelectedOutline: "rgba(9, 105, 218, 0.65)",
  tooltipBg: "#ffffff",
  tooltipBorder: "#d0d7de",
  tooltipFg: "#24292f",
  tooltipItem: "#57606a",
  tooltipSelBg: "rgba(9, 105, 218, 0.1)",
  tooltipSelFg: "#24292f",
  completionDetail: "#8b949e",
  panelBg: "#f6f8fa",
  panelBorder: "#eaeef2",
  panelInputBg: "#ffffff",
  panelInputBorder: "#d0d7de",
  foldBg: "#f6f8fa",
  foldFg: "#57606a",
  danger: "#b91c1c",
  placeholder: "#9aa4b2",
};

const PLAIN_DARK: PlainPalette = {
  fg: "var(--fg-0)",
  gutter: "var(--fg-2)",
  sel: "var(--sel)",
  searchBg: "var(--accent-soft)",
  searchOutline: "var(--accent)",
  searchSelectedOutline: "var(--accent-strong)",
  tooltipBg: "var(--bg-3)",
  tooltipBorder: "var(--line-1)",
  tooltipFg: "var(--fg-0)",
  tooltipItem: "var(--fg-1)",
  tooltipSelBg: "var(--accent-soft)",
  tooltipSelFg: "var(--fg-0)",
  completionDetail: "var(--fg-2)",
  panelBg: "var(--bg-2)",
  panelBorder: "var(--line-0)",
  panelInputBg: "var(--bg-2)",
  panelInputBorder: "var(--line-1)",
  foldBg: "var(--bg-3)",
  foldFg: "var(--fg-1)",
  danger: "var(--danger)",
  placeholder: "var(--fg-2)",
};

// Borderless native-code-area theme: no outline, no visible gutter
// separator, compact subtle line numbers, comfortable padding.
function makePlainTheme(dark: boolean, fontSize: number): Extension {
  const c = dark ? PLAIN_DARK : PLAIN_LIGHT;
  return EditorView.theme(
    {
      "&": {
        backgroundColor: "transparent",
        colorScheme: dark ? "dark" : "light",
        fontSize: `${fontSize}px`,
        color: c.fg,
      },
      ".cm-content": {
        fontFamily: "var(--mono)",
        caretColor: c.fg,
        padding: "10px 12px 10px 0",
      },
      ".cm-scroller": {
        fontFamily: "var(--mono)",
        lineHeight: "1.6",
      },
      ".cm-gutters": {
        backgroundColor: "transparent",
        border: "none",
        color: c.gutter,
      },
      ".cm-lineNumbers .cm-gutterElement": {
        padding: "0 8px 0 16px",
        minWidth: "32px",
      },
      ".cm-activeLine": { backgroundColor: "transparent" },
      ".cm-activeLineGutter": { backgroundColor: "transparent" },
      "&.cm-focused": { outline: "none" },
      "&.cm-focused .cm-selectionBackground, .cm-content ::selection": {
        backgroundColor: c.sel,
      },
      ".cm-cursor, .cm-dropCursor": { borderLeftColor: c.fg },
      ".cm-selectionBackground": { backgroundColor: c.sel },
      ".cm-searchMatch": {
        backgroundColor: c.searchBg,
        outline: `1px solid ${c.searchOutline}`,
      },
      ".cm-searchMatch-selected": {
        backgroundColor: c.searchBg,
        outline: `1px solid ${c.searchSelectedOutline}`,
      },
      ".cm-tooltip": {
        backgroundColor: c.tooltipBg,
        border: `1px solid ${c.tooltipBorder}`,
        fontFamily: "var(--mono)",
        fontSize: "11px",
        color: c.tooltipFg,
        overflow: "hidden",
      },
      ".cm-tooltip.cm-tooltip-autocomplete > ul": {
        fontFamily: "var(--mono)",
        maxHeight: "180px",
      },
      ".cm-tooltip-autocomplete ul li": {
        color: c.tooltipItem,
      },
      ".cm-tooltip-autocomplete ul li[aria-selected]": {
        backgroundColor: c.tooltipSelBg,
        color: c.tooltipSelFg,
      },
      ".cm-completionDetail": {
        color: c.completionDetail,
        fontStyle: "normal",
      },
      ".cm-panels": {
        backgroundColor: c.panelBg,
        color: c.tooltipFg,
        borderBottom: `1px solid ${c.panelBorder}`,
        borderTop: `1px solid ${c.panelBorder}`,
      },
      ".cm-panel input, .cm-panel button": {
        backgroundColor: c.panelInputBg,
        border: `1px solid ${c.panelInputBorder}`,
        color: c.tooltipFg,
        fontSize: "11px",
        borderRadius: "4px",
      },
      ".cm-foldPlaceholder": {
        backgroundColor: c.foldBg,
        border: "none",
        color: c.foldFg,
      },
      ".cm-diagnostic-error": { color: c.danger },
      ".cm-placeholder": { color: c.placeholder, fontStyle: "normal" },
    },
    { dark },
  );
}

export default function CodeEditor({
  value,
  onChange,
  language = "text",
  readOnly = false,
  height,
  lineNumbers: showLineNumbers = false,
  placeholder,
  appearance = "app",
  variables,
}: CodeEditorProps) {
  const theme = useKeel((s) => s.settings.theme);
  const fontSize = useKeel((s) => s.settings.editorFontSize);
  const viewRef = useRef<EditorView | null>(null);

  const { onContextMenu, editMenu } = useEditContextMenu(() => {
    const view = viewRef.current;
    if (!view) return { editable: false };
    const sel = view.state.selection.main;
    const selectedText = view.state.sliceDoc(sel.from, sel.to);
    const copy = () => {
      if (selectedText) void writeText(selectedText).catch(() => {});
    };
    const selectEverything = () => {
      selectAll(view);
      view.focus();
    };
    if (readOnly) {
      return { editable: false, hasSelection: sel.from !== sel.to, copy, selectAll: selectEverything };
    }
    return {
      editable: true,
      hasSelection: sel.from !== sel.to,
      canUndo: undoDepth(view.state) > 0,
      canRedo: redoDepth(view.state) > 0,
      undo: () => {
        undo(view);
        view.focus();
      },
      redo: () => {
        redo(view);
        view.focus();
      },
      cut: () => {
        copy();
        view.dispatch(view.state.replaceSelection(""));
        view.focus();
      },
      copy,
      paste: () => {
        void readText()
          .then((t) => {
            if (!t) return;
            view.dispatch(view.state.replaceSelection(t));
            view.focus();
          })
          .catch(() => {});
      },
      selectAll: selectEverything,
    };
  });

  const extensions = useMemo(() => {
    const dark = theme === "dark";
    const plain = appearance === "editor";
    const exts: Extension[] = [
      plain ? makePlainTheme(dark, fontSize) : makeTheme(dark, fontSize),
      EditorView.lineWrapping,
      syntaxHighlighting(plain && !dark ? lightSyntaxColors : syntaxColors),
      keymap.of(searchKeymap),
      highlightSelectionMatches(),
    ];
    const completion = variableCompletion(variables);
    if (completion) exts.push(completion);
    exts.push(highlightPrevTags());
    if (language === "json") exts.push(json(), jsonLinter);
    else if (language === "yaml") exts.push(yaml());
    else if (language === "javascript") exts.push(javascript());
    if (placeholder && !readOnly) exts.push(cmPlaceholder(placeholder));
    return exts;
  }, [language, theme, fontSize, placeholder, readOnly, appearance, variables]);

  return (
    <div
      className={
        appearance === "editor"
          ? "h-full min-h-0"
          : "rounded border border-line-0 bg-bg-2 focus-within:border-line-focus transition-colors"
      }
      onContextMenu={onContextMenu}
    >
      <CodeMirror
        value={value}
        height={height}
        theme="none"
        extensions={extensions}
        editable={!readOnly}
        readOnly={readOnly}
        basicSetup={{
          lineNumbers: showLineNumbers,
          foldGutter: true,
          highlightActiveLine: false,
          highlightActiveLineGutter: false,
          autocompletion: false,
          highlightSelectionMatches: false,
          searchKeymap: false,
          closeBrackets: false,
        }}
        onChange={onChange}
        onCreateEditor={(view) => {
          viewRef.current = view;
        }}
      />
      {editMenu}
    </div>
  );
}
