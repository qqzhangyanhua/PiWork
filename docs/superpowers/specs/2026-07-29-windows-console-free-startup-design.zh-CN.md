# PiWork Windows 无控制台启动设计

日期：2026-07-29  
状态：待用户审阅

## 目标

PiWork 的 Windows 可执行文件无论以 Debug 还是 Release 模式构建，双击启动时都不得创建或挂起额外的 PowerShell/控制台窗口。通过 `pnpm tauri dev` 主动启动开发环境时，构建工具仍可在原有终端中输出日志，但 PiWork 应用进程不得另行创建控制台窗口。

## 当前原因

Windows 入口当前只在 `not(debug_assertions)` 条件成立时声明 `windows` 子系统。因此 Release 可执行文件使用 GUI 子系统，而 Debug 可执行文件仍使用 Console 子系统。项目 README 又提供了 `pnpm tauri build --debug --bundles nsis` 作为打包命令，所以该构建产物双击启动时会附带控制台窗口。

## 设计

Windows 入口在所有构建配置中声明 GUI 子系统，不再让 `debug_assertions` 决定 Windows 子系统类型。该声明只影响 Windows；其他目标平台保持现状。

这项修改只改变进程启动形态，不改变 Tauri 窗口配置、启动恢复流程、单实例行为、数据库初始化、Work 生命周期或前端 UI。

## 开发体验

- 双击 Debug EXE、Release EXE或安装后的快捷方式：只显示 PiWork 主窗口，不显示额外控制台窗口。
- 执行 `pnpm tauri dev`：开发者打开的终端继续承载 Vite、Cargo 和 Tauri CLI 输出；PiWork 应用不创建第二个控制台窗口。
- 本次不新增独立日志系统。启动准备失败继续使用现有 Windows 原生错误对话框，并提供稳定错误码和数据目录提示。

## 错误处理

现有启动失败处理保持不变：数据库准备、恢复或状态装配失败时，主窗口保持隐藏，并通过原生对话框让用户重试或退出。该路径不依赖控制台，因此隐藏控制台不会移除现有的用户可见错误反馈。

## 验证

1. 更新入口源码回归测试，要求 Windows GUI 子系统声明不再受 `debug_assertions` 限制。
2. 运行 Rust 测试，确认启动编排与单实例行为没有回归。
3. 构建 Debug Windows EXE，并检查 PE Header 的 Subsystem 为 Windows GUI。
4. 双击 Debug EXE进行冒烟验证：只出现 PiWork 主窗口。

## 非目标

- 不在本项中实现 LLM 配置引导。
- 不在本项中调整新建 Work 或工作区 UI。
- 不新增日志查看器、崩溃上报或开发者控制台开关。
- 不改变现有 Release/Debug 优化级别或安装器格式。

