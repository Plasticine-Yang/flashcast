import React from 'react'
import { createRoot } from 'react-dom/client'
import '@fontsource-variable/inter/index.css'
import '@fontsource-variable/jetbrains-mono/index.css'
import './styles/tokens.css'
import './styles/base.css'
import './styles/launcher.css'
import './styles/plugin.css'
import './styles/settings.css'
import { App } from './App'
import { isMock } from './bridge'
import { applyBootTheme } from './theme'

if (isMock) document.documentElement.dataset.preview = 'true'
if (window.matchMedia('(prefers-contrast: more)').matches) {
  document.documentElement.dataset.contrast = 'more'
}

const params = new URLSearchParams(window.location.search)
const platform = params.get('platform') ?? null
if (platform) document.documentElement.dataset.forcedPlatform = platform

// Apply the persisted theme before React renders so a custom pack never flashes
// the bundled Blue Glass on launch.
applyBootTheme()

createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
)
