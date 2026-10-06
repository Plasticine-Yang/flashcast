/** 正文首个非空行作为列表摘要；兼容旧存储中的标题字段。 */
export function memoSummary(body: string): string {
  const firstLine = body.split(/\r?\n/).find((line) => line.trim())?.trim();
  return Array.from(firstLine ?? "空白备忘录").slice(0, 80).join("");
}
