/** 输入中的中英文逗号与顿号分隔标签，保留顺序并去重。 */
export function parseTags(value: string): string[] {
  return [
    ...new Set(
      value
        .split(/[,，、]/)
        .map((part) => part.trim())
        .filter(Boolean),
    ),
  ];
}
