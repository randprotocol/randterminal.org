'use client';
import { useEffect, useRef } from 'react';
import { EditorView, basicSetup } from 'codemirror';
import { EditorState, Compartment } from '@codemirror/state';
import { keymap } from '@codemirror/view';
import { linter, setDiagnostics, type Diagnostic as CmDiagnostic } from '@codemirror/lint';
import { rust } from '@codemirror/lang-rust';
import { solidity } from '@replit/codemirror-lang-solidity';
import type { Diagnostic } from '@/lib/types';

const theme = EditorView.theme({
  '&': { height: '100%', backgroundColor: 'var(--bg-soft)', color: 'var(--text)' },
  '.cm-content': { fontFamily: 'var(--mono)', fontSize: '13px' },
  '.cm-gutters': { backgroundColor: 'var(--bg-soft)', color: 'var(--text-mute)', borderRight: '1px solid var(--border-soft)' },
  '.cm-activeLine, .cm-activeLineGutter': { backgroundColor: 'var(--hl)' },
  '&.cm-focused .cm-cursor': { borderLeftColor: 'var(--accent)' },
});

export function Editor({ value, onChange, language, diagnostics, onRun, readOnly = false }: {
  value: string; onChange: (v: string) => void; language: 'solidity' | 'rust' | 'toml'; diagnostics: Diagnostic[]; onRun: () => void; readOnly?: boolean;
}) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const lang = useRef(new Compartment());
  const cb = useRef({ onChange, onRun });
  cb.current = { onChange, onRun };

  useEffect(() => {
    const v = new EditorView({
      parent: host.current!,
      state: EditorState.create({
        doc: value,
        extensions: [
          basicSetup, theme, linter(() => []),
          lang.current.of(language === 'solidity' ? solidity : language === 'rust' ? rust() : []),
          keymap.of([{ key: 'Mod-Enter', run: () => { cb.current.onRun(); return true; } }]),
          EditorView.updateListener.of((u) => { if (u.docChanged) cb.current.onChange(u.state.doc.toString()); }),
          EditorState.readOnly.of(readOnly),
        ],
      }),
    });
    view.current = v;
    return () => v.destroy();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [language, readOnly]);

  useEffect(() => {
    const v = view.current;
    if (v && v.state.doc.toString() !== value) v.dispatch({ changes: { from: 0, to: v.state.doc.length, insert: value } });
  }, [value]);

  useEffect(() => {
    const v = view.current;
    if (!v) return;
    const doc = v.state.doc;
    const marks: CmDiagnostic[] = diagnostics.filter((d) => d.line >= 1 && d.line <= doc.lines).map((d) => {
      const line = doc.line(d.line);
      const from = Math.min(line.from + Math.max(d.col - 1, 0), line.to);
      return { from, to: line.to > from ? line.to : from, severity: d.severity === 'error' ? 'error' : d.severity === 'warning' ? 'warning' : 'info', message: d.message };
    });
    v.dispatch(setDiagnostics(v.state, marks));
  }, [diagnostics]);

  return <div ref={host} className="h-full overflow-hidden" aria-label="source editor" />;
}
