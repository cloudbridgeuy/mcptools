import React from 'react'
import { createRoot } from 'react-dom/client'
import { Tldraw, loadSnapshot } from 'tldraw'
import { getAssetUrlsByImport } from '@tldraw/assets/imports.vite'
import 'tldraw/tldraw.css'

const assetUrls = getAssetUrlsByImport()

function App() {
  const [editor, setEditor] = React.useState(null)
  const [revision, setRevision] = React.useState(null)
  const [status, setStatus] = React.useState({ state: 'loading', message: 'Loading plan…' })
  const [saving, setSaving] = React.useState(false)

  async function loadPlan(editor) {
    try {
      const response = await fetch(`${window.location.pathname}/plan.tldr`, { cache: 'no-store' })
      if (!response.ok) throw new Error(`Plan request failed (${response.status})`)
      const rev = Number(response.headers.get('x-plan-rev'))
      if (!Number.isSafeInteger(rev) || rev < 1) throw new Error('Missing or invalid plan revision')
      const file = await response.json()
      const store = Object.fromEntries(file.records.map(record => [record.id, record]))
      loadSnapshot(editor.store, { document: { store, schema: file.schema } })
      editor.zoomToFit()
      setEditor(editor)
      setRevision(rev)
      setStatus({ state: 'ready', message: '' })
    } catch (error) {
      setStatus({ state: 'error', message: `Cannot load plan: ${error.message}` })
    }
  }

  async function save() {
    setSaving(true)
    setStatus({ state: 'saving', message: 'Saving…' })
    try {
      const document = editor.store.getStoreSnapshot('document')
      const snapshot = { tldrawFileFormatVersion: 1, schema: document.schema, records: Object.values(document.store) }
      const { blob } = await editor.toImage([...editor.getCurrentPageShapeIds()], { format: 'png', background: true })
      const png = await new Promise((resolve, reject) => {
        const reader = new FileReader()
        reader.onload = () => resolve(reader.result.split(',')[1])
        reader.onerror = () => reject(reader.error)
        reader.readAsDataURL(blob)
      })
      const response = await fetch(`${window.location.pathname}/save`, {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ rev: revision, snapshot, png }),
      })
      if (response.status === 409) {
        setStatus({ state: 'conflict', message: 'Save conflict: the plan changed elsewhere. Reload to see the latest version; your unsaved edits will be lost.' })
        return
      }
      if (response.status === 413) throw new Error('Plan is too large to save (10 MiB limit)')
      if (response.status === 400) throw new Error('The server rejected the plan or image')
      if (!response.ok) throw new Error(`Save failed (${response.status})`)
      const result = await response.json()
      if (!Number.isSafeInteger(result.rev) || result.rev < 1) throw new Error('Invalid saved revision')
      setRevision(result.rev)
      setStatus({ state: 'saved', message: 'Saved', rev: result.rev })
    } catch (error) {
      setStatus({ state: 'error', message: `Cannot save: ${error.message}` })
    } finally {
      setSaving(false)
    }
  }

  return <>
    <Tldraw assetUrls={assetUrls} onMount={editor => { void loadPlan(editor) }} />
    <div style={{ position: 'fixed', top: 'max(8px, env(safe-area-inset-top))', right: 'max(8px, env(safe-area-inset-right))', zIndex: 1000, display: 'flex', alignItems: 'center', gap: 8, maxWidth: 'calc(100vw - 16px)' }}>
      <div id="save-status" role="status" aria-live="polite" data-state={status.state} data-rev={status.rev} style={{ background: 'white', color: '#222', borderRadius: 6, padding: status.message ? 8 : 0 }}>{status.message}</div>
      <button id="save" type="button" onClick={save} disabled={!editor || saving || status.state === 'conflict'} style={{ minWidth: 64, minHeight: 44, padding: '0 16px', borderRadius: 8, border: '1px solid #777', background: 'white', color: '#111', fontWeight: 600 }}>Save</button>
    </div>
  </>
}

createRoot(document.getElementById('root')).render(<App />)
