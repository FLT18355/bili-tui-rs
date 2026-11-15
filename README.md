# bili-tui-rs

Rust + ratatui 的 B 站音视频下载 TUI，调用 `yt-dlp`。Catppuccin Mocha 配色、透明背景、Nerd Font 图标、完整鼠标操作。

## 依赖

- `yt-dlp`、`ffmpeg` 在 `PATH`
- 支持 Nerd Font 的终端

## 运行

```sh
cargo run --release
```

## 设置

按 `s`，或点击工具栏的「清晰度」「音频ID」、右上角 cookies 状态，打开设置窗口：

- **清晰度上限**：`best` 或 `144`-`4320` 的数字，作为视频最大高度。
- **音频ID**：点「获取ID」或按 `Ctrl+G`，先拉取当前链接的音频格式；在列表里 `↑` `↓` 选择，`Enter` 确定，也可点击某一行。列表按码率从高到低排列，首项「自动」表示不指定。留空/自动即交给 yt-dlp 选择。
- **Cookies 路径**：支持 `~/`；留空关闭 cookies。文件不存在时无法保存。

默认读取 `~/cookies.txt`；启动前可用 `BILI_TUI_COOKIES` 指定其他初始路径。

## 输出

默认 `~/Downloads`，可用 `BILI_TUI_OUT` 覆盖：

```sh
BILI_TUI_OUT=~/Videos cargo run --release
```

## 操作

| 输入 | 动作 |
|---|---|
| `Tab` | 输入框/列表焦点切换；设置窗口切字段 |
| `Enter` | 下载；设置窗口保存；音频列表确定 |
| `Ctrl+U` | 清空输入 |
| `↑` `↓` `j` `k` | 选择任务；音频列表选择 ID |
| `s` | 打开设置 |
| `Ctrl+G` | 设置窗口中获取音频 ID 列表 |
| `x` | 取消选中任务（杀死当前 yt-dlp） |
| `Del` | 移除已完成/失败/取消的任务 |
| 鼠标 | 悬停高亮、点击模式/清晰度/音频ID/cookies/任务/取消、滚轮滚动 |
| `q` / `Ctrl+C` | 退出 |
