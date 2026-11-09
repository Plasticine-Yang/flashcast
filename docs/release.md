# 发布与故障排查

tag 是唯一事实来源：`scripts/release.mjs` 升版本 → 提交 → 打附注 tag → 推送；`.github/workflows/release.yml` 由 `v*` tag 触发，先校验 tag 与 `package.json` 版本一致，再并行构建三端，最后由 `publish` job 汇总产物、生成 `SHA256SUMS.txt` 并创建 Release。

## 正常发布

```bash
pnpm release:patch -- --watch   # 0.4.3 → 0.4.4，推完 tag 直接跟到 Release 创建
pnpm release:minor
pnpm release:major
node scripts/release.mjs 1.4.0
```

前置条件：工作区干净、当前在 `main`、`--watch` 需要已登录的 `gh`。脚本会拒绝已存在的同名 tag，并保证版本号只升一次。

`--watch` 会找到 `headBranch === vX.Y.Z` 的 run，执行 `gh run watch --exit-status`：

- 成功：打印 Release 地址后退出 0。
- 失败：打印 `gh run view <id> --log-failed` 与 `gh run rerun <id> --failed` 两条命令后退出 1。

不加 `--watch` 时脚本推完就返回，行为与以前一致。

## 失败的处置

```bash
gh run list --workflow release.yml --limit 3     # 找到这次的 run id
gh run view <id>                                  # 看哪个 job 红了
gh run view <id> --log-failed | tail -80          # 看失败步骤
gh run rerun <id> --failed                        # 只重跑失败的 job
```

`publish` 依赖三个 build job；有 build 失败时它会被跳过，**重跑成功后会自动补跑**，不需要手工建 Release。

| 现象 | 处置 |
|---|---|
| 依赖安装 / 打包步骤报 504、连接重置 | 流水线已内置 3 次、每次 60s 退避重试；仍失败时等 5–10 分钟再 `gh run rerun --failed` |
| `tag 与 package.json 版本不一致` | 说明发布没有走脚本。停手，不要手改版本或挪 tag；按维护者意见处理 |
| macOS 产物打不开 / arm64 秒退 | 检查 `scripts/after-pack.mjs` 的 ad-hoc 签名是否执行（日志里搜 `codesign`） |
| Release 已创建但缺个别资产 | `gh release upload <tag> <file> --clobber` 补齐，不要删 tag 重发同版本 |
| 更新器提示下载 504 | 多半是 GitHub 资产 CDN 抖动或新资产还在复制，等几分钟重试；见下节 |

**不要为了修 CI 去挪或重打已推送的 tag。** 版本号已经对外可见，挪 tag 会让别人拿到的产物和 tag 内容对不上。失败就重跑；确实需要新内容时发下一个 patch。

## 发布后验证

```bash
gh run view <id> --json conclusion --jq .conclusion          # success
gh release view v0.4.4 --json assets --jq '.assets[].name'   # 三端齐全 + SHA256SUMS.txt
curl -sIL -o /dev/null -w '%{http_code}\n' <asset-url>       # 关键资产应为 200
```

`SHA256SUMS.txt` 由 `publish` job 用 `sha256sum *` 生成；更新器下载后按它校验，所以资产改名后要一并更新。

## 已知坑

- **Electron 下载与 Release 资产共用 GitHub 的 CDN**：`@electron/get`、electron-builder 和用户的自动更新都从 `github.com/.../releases/download/...` 取文件。该 CDN 偶发局部 504（v0.4.4 发布时 `electron-v45.2.1-linux-x64.zip` 连续 3 次 504，第 4 次恢复）。CI 已通过 `scripts/postinstall.mjs` 跳过 `pnpm install` 里那份用不到的 Electron 运行时，但 electron-builder 打包时仍会下载。
- **没有签名证书**：macOS 只补 ad-hoc 签名（不公证，首次打开要右键），Windows 会触发 SmartScreen。
- **新 Release 的资产有复制延迟**：刚创建就点更新可能拿到 404/504，等几分钟重试即可。
