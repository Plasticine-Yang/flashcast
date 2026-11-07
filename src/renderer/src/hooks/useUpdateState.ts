import { useCallback, useEffect, useState } from 'react'
import type { UpdateState } from '@shared/types'
import { api } from '../bridge'

/**
 * Shared update state: the main process owns the lifecycle and broadcasts every
 * transition, so both windows stay in sync without polling.
 */
export function useUpdateState(): {
  update: UpdateState | null
  loaded: boolean
  refresh: () => void
} {
  const [update, setUpdate] = useState<UpdateState | null>(null)
  const [loaded, setLoaded] = useState(false)

  useEffect(() => {
    let cancelled = false
    const off = api.on('updates:changed', (next) => {
      if (!cancelled) {
        setUpdate(next)
        setLoaded(true)
      }
    })
    void api
      .getUpdateState()
      .then((next) => {
        if (cancelled) return
        setUpdate(next)
        setLoaded(true)
      })
      .catch(() => {
        if (!cancelled) setLoaded(true)
      })
    return () => {
      cancelled = true
      off()
    }
  }, [])

  const refresh = useCallback(() => {
    void api
      .getUpdateState()
      .then((next) => setUpdate(next))
      .catch(() => undefined)
  }, [])

  return { update, loaded, refresh }
}
