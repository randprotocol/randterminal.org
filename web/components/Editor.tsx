'use client';
import { useEffect, useRef } from 'react';
import { EditorView, basicSetup } from 'codemirror';
import { EditorState, Compartment } from '@codemirror/state';
import { keymap } from '@codemirror/view';
import { Prec } from '@codemirror/state';
import { HighlightStyle, syntaxHighlighting } from '@codemirror/language';
import { tags as t } from '@lezer/highlight';
import { lintGutter, setDiagnostics, type Diagnostic as CmDiagnostic } from '@codemirror/lint';
import { rust } from '@codemirror/lang-rust';
import { solidity } from '@replit/codemirror-lang-solidity';
import type { Diagnostic } from '@/lib/types';

const theme = EditorView.theme({
  '&': { height: '100%', backgroundColor: 'var(--bg)', color: 'var(--text)' },
  '.cm-scroller': { fontFamily: 'var(--mono)', lineHeight: '1.6' },
  '.cm-content': { fontSize: '13px', caretColor: 'var(--accent)' },
  '.cm-gutters': { backgroundColor: 'var(--bg)', color: 'var(--text-mute)', border: 'none' },
  '.cm-activeLine': { backgroundColor: 'transparent' },
  '&.cm-focused .cm-activeLine': { backgroundColor: 'var(--hl)' },
  '.cm-activeLineGutter': { backgroundColor: 'transparent', color: 'var(--text-strong)' },
  '.cm-selectionBackground, &.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground': { backgroundColor: 'var(--hl)' },
  '&.cm-focused .cm-cursor': { borderLeftColor: 'var(--accent)' },
});

// Colours are variables, so one definition follows the theme switch without rebuilding the editor.
const highlight = HighlightStyle.define([
  { tag: [t.comment, t.lineComment, t.blockComment, t.docComment], color: 'var(--syn-comment)' },
  { tag: [t.keyword, t.modifier, t.controlKeyword, t.operatorKeyword, t.definitionKeyword, t.moduleKeyword], color: 'var(--syn-keyword)' },
  { tag: [t.string, t.special(t.string), t.character], color: 'var(--syn-string)' },
  { tag: [t.number, t.bool, t.null, t.atom], color: 'var(--syn-number)' },
  { tag: [t.typeName, t.className, t.namespace, t.standard(t.typeName)], color: 'var(--syn-type)' },
  { tag: [t.function(t.variableName), t.function(t.definition(t.variableName)), t.macroName], color: 'var(--syn-fn)', fontWeight: '500' },
  { tag: t.invalid, color: 'var(--negative)' },
]);

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
          syntaxHighlighting(highlight), basicSetup, theme, lintGutter(),
          lang.current.of(language === 'solidity' ? solidity : language === 'rust' ? rust() : []),
          // Highest precedence: the default keymap binds Mod-Enter to insertBlankLine. Returning true makes
          // CodeMirror preventDefault, so Shell's window handler skips the event: exactly one run fires.
          Prec.highest(keymap.of([{ key: 'Mod-Enter', run: () => { cb.current.onRun(); return true; } }])),
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
  }, [diagnostics, language, readOnly]);

  return <div ref={host} className="h-full overflow-hidden" aria-label="source editor" />;
}
