# Hello Flashcast

A minimal external plugin for Flashcast. It demonstrates:

- `plugin.json` metadata, keywords, icon and config defaults
- a declarative settings schema (text + number)
- `search(ctx, query)` returning list items
- `activate(ctx, item)` using host services

Install it from **设置 → 插件 → 安装插件** by entering this directory path.
The plugin runs in the host main process with your user privileges; only install
plugins you trust.
