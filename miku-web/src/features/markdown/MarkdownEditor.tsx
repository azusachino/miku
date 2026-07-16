import { useEffect, useRef, useState } from "react";
import { markdown } from "@codemirror/lang-markdown";
import { EditorState, type Extension } from "@codemirror/state";
import { minimalSetup, EditorView } from "codemirror";
import type { Theme } from "../../shared/ui";

type MarkdownEditorProps = {
  noteId: string;
  value: string;
  readOnly?: boolean;
  theme?: Theme;
  onChange?: (value: string) => void;
};

function editorTheme(): Extension {
  return EditorView.theme({
    "&": { backgroundColor: "var(--surface-code)", color: "var(--text)" },
    ".cm-content": { caretColor: "var(--accent)" },
    ".cm-cursor, .cm-dropCursor": { borderLeftColor: "var(--accent)" },
    ".cm-selectionBackground, ::selection": { backgroundColor: "var(--accent-soft)" },
    ".cm-gutters": { color: "var(--faint)", backgroundColor: "var(--surface-code)" },
    ".cm-activeLine, .cm-activeLineGutter": { backgroundColor: "var(--panel-2)" },
    ".cm-scroller": { fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace" }
  });
}

/**
 * Source-first editor boundary. CodeMirror owns the document after mount;
 * React only replaces it when the selected note changes. This avoids copying
 * the complete document through component state on every keystroke.
 */
export function MarkdownEditor({ noteId, value, readOnly = false, theme = "dark", onChange }: MarkdownEditorProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const noteIdRef = useRef(noteId);
  const onChangeRef = useRef(onChange);
  const syncingRef = useRef(false);
  const [dirty, setDirty] = useState(false);

  onChangeRef.current = onChange;

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;

    const startState = EditorState.create({
      doc: value,
      extensions: [
        minimalSetup,
        markdown(),
        editorTheme(),
        EditorView.editable.of(!readOnly),
        EditorState.readOnly.of(readOnly),
        EditorView.updateListener.of((update) => {
          if (!update.docChanged || syncingRef.current) return;
          setDirty(true);
          onChangeRef.current?.(update.state.doc.toString());
        })
      ]
    });
    const view = new EditorView({ state: startState, parent: host });
    viewRef.current = view;

    return () => {
      view.destroy();
      viewRef.current = null;
    };
  }, [readOnly]);

  useEffect(() => {
    const view = viewRef.current;
    if (!view || noteIdRef.current === noteId) return;
    noteIdRef.current = noteId;
    syncingRef.current = true;
    view.dispatch({
      changes: { from: 0, to: view.state.doc.length, insert: value },
      selection: { anchor: 0 }
    });
    syncingRef.current = false;
    setDirty(false);
  }, [noteId, value]);

  return (
    <div className="markdown-editor" data-note-id={noteId} data-dirty={dirty || undefined}>
      <div className="markdown-editor-header">
        <span>
          <span className="editor-dot" /> Markdown source
        </span>
        <span className="editor-state">{readOnly ? "readonly" : dirty ? "unsaved" : "saved"}</span>
      </div>
      <div ref={hostRef} className="markdown-editor-surface" aria-label="Markdown source editor" />
    </div>
  );
}

export default MarkdownEditor;
