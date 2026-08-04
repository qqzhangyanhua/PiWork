# PiWork 多模型配置实施计划

## 目标

将设置中的单一模型表单改造成由 Rust 管理的多配置列表。用户可查看已保存配置、切换当前配置、使用安全存储中的凭据测试连接，并新增配置。首次启动流程保持可用。

## 契约

- 配置元数据由 Rust 持久化；前端只投影列表与状态。
- 每个配置使用稳定 ID，Credential Manager 以 ID 隔离 API Key。
- 列表不返回 API Key，只返回是否存在安全凭据。
- Runtime 始终读取当前激活配置。
- 旧的单配置记录与旧凭据键在首次读取时兼容迁移。

## 测试驱动步骤

1. Rust 测试定义多配置保存、列表、激活、已保存配置连接测试和旧配置兼容。
2. 扩展 repository、CredentialVault、ModelService 与 Tauri commands。
3. 前端测试定义模型列表、当前标记、切换、测试连接和新增表单。
4. 扩展 tauriClient、mock、设置页与中英文文案。
5. 用受约束的灰阶、轻边框和紧凑行高完成设置页视觉，并验证窄窗口。
6. 执行 typecheck、vitest、build 与 cargo test。
