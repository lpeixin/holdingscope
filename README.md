# HoldingScope

HoldingScope 是本地桌面应用，用于浏览 [Dataroma 跟踪的持仓大户](https://www.dataroma.com/m/managers.php)。首次启动时读取系统 WebView 的首选语言。中文首选项使用简体中文，其余语言使用英文。工具栏可切换语言或恢复跟随系统，手动选择保存在本机。

## 运行与打包

安装 Node.js、Rust 和对应操作系统的 [Tauri 2 系统依赖](https://v2.tauri.app/start/prerequisites/)。在项目目录运行：

```sh
npm ci
npm run tauri dev
```

在目标操作系统上运行 `npm run tauri build` 打包。产物位于 `src-tauri/target/release/bundle/`。该命令会将 Rust 编译产物中的本机源码路径替换为通用路径。

## 数据与同步

- 启动时先读取本地目录和已下载的组合，再通过 [RSS](https://feeds.feedburner.com/dataroma)、[首页更新记录](https://www.dataroma.com/m/home.php) 和大户目录检查变更。首次打开某个组合时下载完整明细，已缓存的组合发现更新后会在后台刷新。
- Rust 获取和解析 Dataroma 公布的持仓、季度增减持及历史组合。大组合的持仓会自动读完来源分页，变动页可按需加载更早的记录。React 通过 Tauri commands 使用这些结构化数据，图表由 Apache ECharts 在本机绘制。
- 投资者中文名按 Dataroma 固定 ID 随应用发布，发行人常用译名及行业译名使用本地词表。搜索接受中英文名称，证券代码保持原样。新收录或尚无译名的专有名称会显示来源原文。SQLite 继续保存英文原文、证券代码和数值，同一份快照即可用于两套界面。
- 当前持仓可按证券代码、权重、股数、期末价和市值排序。历史页可比较不同季度的权重，同一组合下载过的季度使用完整 SQLite 快照，其他季度使用 Dataroma 公布的前 20 大持仓。
- SQLite 数据库 `holdingscope.sqlite3` 存放在系统的应用数据目录。断网时仍可打开已下载的组合，重新联网后可点击「同步数据」。

## 验证

```sh
npm run build
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml live_dataroma -- --ignored --nocapture
```

最后一条命令会访问 Dataroma 公开数据源，检查目录、持仓、变动和历史数据是否仍能解析。
