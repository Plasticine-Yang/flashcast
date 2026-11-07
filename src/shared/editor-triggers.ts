/**
 * Trigger words are the single-token entries that take a plugin straight from
 * the global search into a scoped page (`trae corehr`). They are configured per
 * editor variant, so validation has to be shared by the main process and the
 * preview mock, and enforced on save rather than while typing.
 */
export const TRIGGER_PATTERN = /^[\p{L}\p{N}._-]{1,16}$/u

/** Words that already mean something in the launcher. */
export const RESERVED_TRIGGERS = new Set(['settings', 'quit', 'update', 'refresh', 'help'])

export interface TriggerDraft {
  enabled: boolean
  trigger: string
}

export interface TriggerValidation {
  /** Accepted values, in the order of `ids`. Rejected entries keep their old trigger. */
  accepted: Record<string, TriggerDraft>
  /** Human-readable reason per rejected variant id. */
  errors: Record<string, string>
}

export function normalizeTrigger(value: string): string {
  return value.trim().toLowerCase()
}

/**
 * Validates drafts for one plugin. An empty trigger means "no global entry" and
 * is always allowed. A rejected trigger is reported and the variant keeps the
 * trigger it already had, so a typo never silently removes a working entry.
 */
export function validateTriggers(
  ids: string[],
  drafts: Record<string, TriggerDraft>,
  current: Record<string, TriggerDraft>,
  otherTriggers: Set<string>
): TriggerValidation {
  const accepted: Record<string, TriggerDraft> = {}
  const errors: Record<string, string> = {}
  const used = new Set<string>()

  for (const id of ids) {
    const previous = current[id] ?? { enabled: true, trigger: '' }
    const draft = drafts[id] ?? previous
    const enabled = typeof draft.enabled === 'boolean' ? draft.enabled : previous.enabled
    const trigger = normalizeTrigger(draft.trigger ?? '')

    if (!trigger) {
      accepted[id] = { enabled, trigger: '' }
      continue
    }

    const reject = (message: string): void => {
      errors[id] = message
      accepted[id] = { enabled, trigger: normalizeTrigger(previous.trigger) }
    }

    if (!TRIGGER_PATTERN.test(trigger)) {
      reject('触发词只能包含字母、数字、汉字、点、下划线或短横线（1–16 个字符）')
      continue
    }
    if (RESERVED_TRIGGERS.has(trigger)) {
      reject(`「${trigger}」是系统保留字，请换一个`)
      continue
    }
    if (used.has(trigger)) {
      reject(`触发词「${trigger}」重复了`)
      continue
    }
    if (otherTriggers.has(trigger)) {
      reject(`触发词「${trigger}」已被其他插件使用`)
      continue
    }

    used.add(trigger)
    accepted[id] = { enabled, trigger }
  }

  return { accepted, errors }
}
