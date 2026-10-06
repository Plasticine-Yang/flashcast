const paths: Record<string, string> = {
  back: "m14 6-6 6 6 6",
  edit: "m15 4 5 5 M4 20l4-1L20 7l-4-4L4 15z",
  trash: "M4 6h16 M9 6V3h6v3 M6 6l1 15h10l1-15 M10 10v7 M14 10v7",
  application: "M4 4h6v6H4z M14 4h6v6h-6z M4 14h6v6H4z M14 14h6v6h-6z",
  browser: "M3 5h18v15H3z M3 9h18 M6 7h1 M9 7h1",
  editor: "m8 7-5 5 5 5 M16 7l5 5-5 5 M14 4l-4 16",
  calculator: "M6 3h12v18H6z M9 6h6 M9 11h1 M14 11h1 M9 15h1 M14 15h1 M9 18h1 M14 18h1",
  command: "m5 6 6 6-6 6 M13 18h6",
  memo: "M6 3h12v18H6z M9 7h6 M9 11h6 M9 15h4",
  clipboardEntry: "M8 5H5v16h14V5h-3 M8 3h8v4H8z M8 12h8 M8 16h5",
  bookmark: "M5 3h14v18l-7-4-7 4z",
  hotkey: "M3 6h18v12H3z M6 10h1 M10 10h1 M14 10h1 M18 10h1 M7 14h10",
  theme: "M12 3a9 9 0 1 0 0 18V3z M12 3a9 9 0 0 1 0 18",
  plugins: "M8 3v5H3v8h5v5h8v-5h5V8h-5V3z",
  workspace: "M3 6h7l2 2h9v12H3z",
  sync: "M4 9a8 8 0 0 1 14-4l2 3 M20 3v5h-5 M20 15A8 8 0 0 1 6 19l-2-3 M4 21v-5h5",
  changes: "M7 3v12 M17 9v12 M7 9h6a4 4 0 0 1 4 4 M5 3h4 M15 21h4",
  capabilities: "M3 4h18v13H3z M8 21h8 M12 17v4",
};
export function Glyph({ name, className = "" }: { name: string; className?: string }) {
  return <svg className={`glyph ${className}`} width="18" height="18" viewBox="0 0 24 24" fill="none"
    stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d={paths[name] ?? paths.application} />
  </svg>;
}
