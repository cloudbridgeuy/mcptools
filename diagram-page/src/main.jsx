import React from 'react'
import { createRoot } from 'react-dom/client'
import { Tldraw, loadSnapshot } from 'tldraw'
import 'tldraw/tldraw.css'

async function loadPlan(editor) {
  try {
    const response = await fetch(`${window.location.pathname}/plan.tldr`, { cache: 'no-store' })
    if (!response.ok) throw new Error(`Plan request failed (${response.status})`)
    const file = await response.json()
    const store = Object.fromEntries(file.records.map(record => [record.id, record]))
    loadSnapshot(editor.store, { document: { store, schema: file.schema } })
    editor.zoomToFit()
  } catch (error) {
    const message = document.getElementById('error')
    message.textContent = `Cannot load plan: ${error.message}`
    message.hidden = false
  }
}

createRoot(document.getElementById('root')).render(
  <Tldraw onMount={editor => { void loadPlan(editor) }} />,
)
