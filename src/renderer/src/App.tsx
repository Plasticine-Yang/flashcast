import { Launcher } from './launcher/Launcher'
import { SettingsApp } from './settings/SettingsApp'

const params = new URLSearchParams(window.location.search)
const windowRole = params.get('window') === 'settings' ? 'settings' : 'launcher'

export function App(): React.JSX.Element {
  return windowRole === 'settings' ? <SettingsApp /> : <Launcher />
}
