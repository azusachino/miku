import { useEffect, useRef, useState } from "react";
import { markdown } from "@codemirror/lang-markdown";
import { EditorState, type Extension } from "@codemirror/state";
import { minimalSetup } from "codemirror";
import { EditorView, keymap } from "@codemirror/view";
import type { Theme } from "../../shared/ui";

type MarkdownEditorProps = {
  noteId: string;
  value: string;
  readOnly?: boolean;
  theme?: Theme;
  onChange?: (value: string) => void;
  onSave?: () => void;
};

function editorTheme(): Extension {
  return EditorView.theme({
    "&": { backgroundColor: "transparent", color: "var(--text)" },
    "&.cm-focused": { outline: "none" },
    ".cm-content": {
      caretColor: "var(--accent)",
      fontFamily: "var(--font-mono, ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace)",
      fontSize: "0.86rem",
      lineHeight: "1.7"
    },
    ".cm-line": { padding: "0 4px" },
    ".cm-cursor, .cm-dropCursor": { borderLeftColor: "var(--accent)", borderLeftWidth: "2px" },
    ".cm-selectionBackground, ::selection": { backgroundColor: "var(--accent-soft) !important" },
    ".cm-gutters": {
      color: "var(--faint)",
      backgroundColor: "transparent",
      borderRight: "1px solid color-mix(in srgb, var(--line) 40%, transparent)",
      paddingRight: "6px",
      marginRight: "12px"
    },
    ".cm-activeLine": { backgroundColor: "color-mix(in srgb, var(--panel-2) 50%, transparent)" },
    ".cm-activeLineGutter": { backgroundColor: "color-mix(in srgb, var(--panel-2) 70%, transparent)", color: "var(--muted)" },
    ".cm-scroller": {
      fontFamily: "var(--font-mono, ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace)",
      overflow: "visible"
    }
  });
}

/**
 * Source-first editor boundary. CodeMirror owns the document after mount;
 * React only replaces it when the selected note changes. This avoids copying
 * the complete document through component state on every keystroke.
 */
export function MarkdownEditor({ noteId, value, readOnly = false, theme = "dark", onChange, onSave }: MarkdownEditorProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const noteIdRef = useRef(noteId);
  const onChangeRef = useRef(onChange);
  const onSaveRef = useRef(onSave);
  const syncingRef = useRef(false);
  const [dirty, setDirty] = useState(false);

  onChangeRef.current = onChange;
  onSaveRef.current = onSave;

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;

    const startState = EditorState.create({
      doc: value,
      extensions: [
        minimalSetup,
        markdown(),
        EditorView.lineWrapping,
        editorTheme(),
        EditorView.editable.of(!readOnly),
        EditorState.readOnly.of(readOnly),
        keymap.of([
          {
            key: "Mod-s",
            run: () => {
              onSaveRef.current?.();
              return true;
            }
          }
        ]),
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
      <div ref={hostRef} className="markdown-editor-surface" aria-label="Markdown source editor" />
    </div>
  );
}

export default MarkdownEditor;
