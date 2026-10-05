import { readFileSync } from "node:fs";

export function validateMetadata(tag) {
  if (!/^v\d+\.\d+\.\d+$/.test(tag ?? "")) throw new Error("需要 vX.Y.Z 版本标签");
  const version = tag.slice(1);
  const cargo = readFileSync("Cargo.toml", "utf8").split("[workspace.package]")[1]?.split(/\n\[/)[0];
  const actual = [cargo?.match(/^version\s*=\s*"([^"]+)"/m)?.[1],
    JSON.parse(readFileSync("package.json", "utf8")).version,
    JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8")).version];
  if (actual.some(item => item !== version)) throw new Error(`版本不一致：标签 ${version}，元数据 ${actual.join(" / ")}`);
  if (!readFileSync(`docs/release/${tag}.md`, "utf8").trim()) throw new Error("发布说明为空");
}
