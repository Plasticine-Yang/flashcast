/**
 * Hello Flashcast — a dependency-free example external plugin.
 *
 * The host executes this file in the main process. `ctx` exposes config,
 * clipboard/openExternal services, lifecycle disposers and logging.
 */
module.exports = {
  async search(ctx, query) {
    const config = await ctx.config.get()
    const greeting = String(config.greeting || '你好')
    const count = Math.min(10, Math.max(1, Number(config.count) || 5))
    const items = Array.from({ length: count }, (_, index) => ({
      id: `greeting-${index + 1}`,
      title: `${greeting} #${index + 1}`,
      subtitle: '来自外部插件的示例条目',
      badge: '示例'
    }))

    const needle = query.trim().toLowerCase()
    if (!needle) return items
    return items.filter((item) =>
      `${item.title} ${item.subtitle} ${item.badge}`.toLowerCase().includes(needle)
    )
  },

  async activate(ctx, item) {
    await ctx.services.copyText(item.title)
    ctx.services.toast(`已复制「${item.title}」`, 'success')
    return { ok: true, close: true }
  }
}
