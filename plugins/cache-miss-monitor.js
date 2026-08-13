const CACHE_EXPIRY_MS = 5 * 60 * 1000
const DEFAULT_MODE = "warn"
const VALID_MODES = new Set(["block", "warn"])

export function cacheMissMode(value = process.env.OPENCODE_CACHE_MISS_MODE) {
  return VALID_MODES.has(value) ? value : DEFAULT_MODE
}

export function isShortIntervalMiss(previous, current) {
  if (!previous || current.cacheRead !== 0) return false
  const elapsed = current.completed - previous.completed
  return elapsed >= 0 && elapsed <= CACHE_EXPIRY_MS
}

function assistantUsage(info) {
  const cache = info?.tokens?.cache
  if (!info || info.role !== "assistant" || info.time?.completed == null) return null
  if (typeof cache?.read !== "number") return null
  return {
    completed: info.time.completed,
    cacheRead: cache.read,
    providerID: info.providerID,
    modelID: info.modelID,
    sessionID: info.sessionID,
  }
}

export const CacheMissMonitor = async ({ client }) => {
  const mode = cacheMissMode()
  const lastRequest = new Map()
  const handledMessages = new Set()

  return {
    event: async ({ event }) => {
      if (event.type !== "message.updated") return

      const info = event.properties?.info
      const usage = assistantUsage(info)
      if (!usage || handledMessages.has(info.id)) return
      handledMessages.add(info.id)

      const key = `${usage.sessionID}:${usage.providerID}:${usage.modelID}`
      const previous = lastRequest.get(key)
      lastRequest.set(key, usage)

      if (!isShortIntervalMiss(previous, usage)) return

      const message =
        `Provider cache miss detected within 5 minutes for ${usage.providerID}/${usage.modelID}. ` +
        "The provider may not be honoring prompt caching."

      try {
        await client.tui.showToast({
          body: {
            title: "Prompt cache warning",
            message,
            variant: mode === "block" ? "error" : "warning",
            duration: 10000,
          },
        })
      } catch (error) {
        console.error("[cache-miss-monitor] failed to show warning:", error?.message ?? error)
      }

      if (mode !== "block") return

      try {
        await client.session.abort({ path: { id: usage.sessionID } })
      } catch (error) {
        console.error("[cache-miss-monitor] failed to abort session:", error?.message ?? error)
      }
    },
  }
}
