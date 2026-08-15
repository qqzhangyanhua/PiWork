import type { CapabilityPackStatus } from "../../bindings";

export type CapabilityPriority = "P0" | "P1" | "P2";

export type AgentCapabilityDomainId =
  | "innovation-service"
  | "sales-operations"
  | "requirements-assessment"
  | "quote-finance"
  | "project-delivery"
  | "engineering-rd"
  | "manufacturing-quality"
  | "delivery-knowledge"
  | "ai-governance";

export type AgentCapability = {
  id: number;
  catalogId: `catalog-capability:${string}`;
  status: CapabilityPackStatus;
  capabilityPackId: string;
  name: string;
  domainId: AgentCapabilityDomainId;
  priority: CapabilityPriority;
  audiences: string[];
  coreCapability: string;
  outputs: string[];
  implementation: string;
  suggestedInputs: string[];
};

export type CapabilityFilters = {
  query: string;
  domainId: AgentCapabilityDomainId | "all";
  priority: CapabilityPriority | "all";
};

export const AGENT_CAPABILITY_DOMAINS: ReadonlyArray<{
  id: AgentCapabilityDomainId;
  name: string;
  count: number;
}> = [
  { id: "innovation-service", name: "研创用户与创新服务", count: 8 },
  { id: "sales-operations", name: "客户、线索与销售运营", count: 7 },
  { id: "requirements-assessment", name: "需求、评估与承接决策", count: 9 },
  { id: "quote-finance", name: "报价、合同、支付与财务", count: 12 },
  { id: "project-delivery", name: "项目、协作与履约管理", count: 10 },
  { id: "engineering-rd", name: "专业工程研发", count: 14 },
  { id: "manufacturing-quality", name: "采购、试制、生产与质量", count: 16 },
  { id: "delivery-knowledge", name: "交付、售后与知识资产", count: 8 },
  { id: "ai-governance", name: "AI中台运营、治理与安全", count: 12 },
];

export const AGENT_CAPABILITY_PATHS = [
  {
    id: "requirements-to-quote",
    title: "需求到报价闭环",
    description: "从资料接收、需求澄清和技术评估推进到成本与报价预审。",
    capabilityIds: [1, 2, 9, 14, 16, 17, 20, 21, 22, 23, 25, 26, 28, 30, 33, 45, 85, 88, 89, 90, 92, 93, 95],
  },
  {
    id: "project-to-prototype",
    title: "项目到样机闭环",
    description: "把项目计划、专业研发、试制、变更和验收组织成持续协作。",
    capabilityIds: [19, 37, 38, 39, 40, 42, 43, 44, 46, 49, 50, 54, 55, 56, 57, 58, 66, 67, 72, 77],
  },
  {
    id: "process-to-manufacturing",
    title: "工艺到制造闭环",
    description: "覆盖供应商、询价、工艺、生产、质量、成本和知识回流。",
    capabilityIds: [61, 62, 63, 64, 68, 69, 70, 73, 74, 76, 82, 84],
  },
  {
    id: "platform-productization",
    title: "平台产品化",
    description: "把成熟能力沉淀为可配置、可评测、可治理的场景能力库。",
    capabilityIds: [3, 5, 6, 7, 8, 86, 87, 91, 94, 96],
  },
] as const;

type CapabilityRow = readonly [
  id: number,
  name: string,
  domainId: AgentCapabilityDomainId,
  audiences: string,
  coreCapability: string,
  outputs: string,
  implementation: string,
  priority: CapabilityPriority,
];

export function getCatalogCapabilityId(id: number): AgentCapability["catalogId"] {
  return `catalog-capability:${String(id).padStart(3, "0")}`;
}

const ROWS: CapabilityRow[] = [
  [1, "智能任务分流智能体", "innovation-service", "外部用户、平台运营", "判断用户是在提想法、交资料、查项目、询价还是寻求生态服务", "任务类型、推荐路径、工作流入口", "意图分类 + 规则路由", "P0"],
  [2, "多模态资料接收智能体", "innovation-service", "外部用户、商务", "接收文字、图片、需求书、图纸、BOM、录音和视频摘要", "文件摘要、字段、附件分类、缺失提示", "OCR/ASR/多模态模型 + 解析器", "P0"],
  [3, "创新想法理解智能体", "innovation-service", "创业者、科研团队、企业创新人员", "将模糊想法整理为用户、场景、问题和产品目标", "创新机会卡、初步产品事实", "大模型 + 创新方法模板", "P1"],
  [4, "概念方案生成智能体", "innovation-service", "外部用户、产品经理、设计师", "形成产品方向、概念图、功能组合、候选 BOM 和成本周期区间", "概念方向、概念图、候选 BOM", "图像生成 + 方案模板 + 成本规则", "P1"],
  [5, "市场机会研究智能体", "innovation-service", "创新团队、产品经理", "检索市场、竞品、应用案例、客户和商业机会", "市场简报、竞品矩阵、机会判断", "联网检索 + RAG + 数据源接口", "P2"],
  [6, "知识产权检索智能体", "innovation-service", "创新团队、知识产权人员", "检索专利、商标和公开技术路线，提示潜在冲突", "专利检索摘要、相似方案、风险项", "专利数据接口 + 语义检索", "P2"],
  [7, "政策与资金匹配智能体", "innovation-service", "创新团队、园区运营", "按行业、地区、阶段和主体匹配政策、补贴和基金", "政策清单、申报条件、材料建议", "规则匹配 + 政策知识库", "P2"],
  [8, "生态服务推荐智能体", "innovation-service", "外部用户、平台运营", "推荐设计、检测、认证、知识产权、融资、工厂和渠道服务", "服务组合、服务商候选、推荐理由", "服务目录 + 画像匹配 + 排序", "P2"],
  [9, "线索分析智能体", "sales-operations", "销售、商务", "识别客户类型、需求价值、资料成熟度、优先级和红线", "线索摘要、优先级、受理建议", "规则 + 大模型结构化抽取", "P0"],
  [10, "客户主体核验智能体", "sales-operations", "销售、商务、风控", "核验企业主体、联系人、经营状态、行业和资质", "主体档案、核验结果、风险提示", "工商接口 + 规则校验", "P1"],
  [11, "客户画像智能体", "sales-operations", "销售、商务、管理者", "综合企业属性、历史互动、项目结果、付款和复购形成画像", "价值评分、价格敏感度、服务复杂度", "特征评分 + 大模型解释", "P1"],
  [12, "商机资格判断智能体", "sales-operations", "销售、商务", "判断是否有预算、决策人、时间、真实需求和后续量产潜力", "商机阶段、赢单概率、缺失条件", "规则评分 + 历史结果模型", "P1"],
  [13, "销售跟进策略智能体", "sales-operations", "销售、商务", "根据商机状态推荐下一动作、话术、材料和跟进时间", "跟进建议、待办、沟通草稿", "CRM事件 + 策略规则 + 生成模型", "P1"],
  [14, "会议与沟通纪要智能体", "sales-operations", "销售、商务、客户", "从录音和聊天中提取事实、承诺、问题、责任人和截止时间", "纪要、行动项、需求更新建议", "ASR + 结构化抽取", "P0"],
  [15, "NDA与资料准备智能体", "sales-operations", "商务、客户", "判断保密阶段，生成 NDA 草稿并检查详细资料是否可获取", "NDA草稿、资料清单、权限提示", "模板引擎 + 规则 + 电子签接口", "P1"],
  [16, "需求澄清智能体", "requirements-assessment", "客户、商务、工程师", "根据资料成熟度动态追问影响工程实现的关键问题", "产品事实、问题、缺失和冲突", "RAG + 动态问卷 + Schema", "P0"],
  [17, "产品事实管理智能体", "requirements-assessment", "客户、产品、商务、PM", "管理目标、用户、场景、范围、约束、交付和来源版本", "产品事实 Vn、确认记录、来源", "事实 Schema + 版本引擎", "P0"],
  [18, "需求冲突与变更识别智能体", "requirements-assessment", "商务、PM、工程师", "比较多轮需求、附件和会议，识别新增、删除、冲突和影响", "变更单、差异、影响范围", "语义 Diff + 规则检查", "P1"],
  [19, "服务拆解智能体", "requirements-assessment", "商务、PM、评估工程师", "将整机需求拆成外观、结构、机、电、软、测试、采购和试制服务包", "服务范围树、工作包、交付清单", "模板库 + 任务图生成", "P1"],
  [20, "评估工程师匹配智能体", "requirements-assessment", "商务、研发负责人", "按技能、历史项目、当前负荷和协同要求推荐评估人员", "主评估人、协同人、匹配依据", "技能图谱 + 排序模型", "P0"],
  [21, "相似项目检索智能体", "requirements-assessment", "工程师、商务", "从历史需求、方案、报价和结果中寻找可对照项目", "相似案例、差异、结果和风险", "向量检索 + 结构化过滤", "P0"],
  [22, "技术方案检索智能体", "requirements-assessment", "评估工程师、项目工程师", "检索内部成熟方案、外部模块、标准和失败经验", "技术参考、来源、适用条件", "内外部 RAG + 工具搜索", "P0"],
  [23, "技术可行性评估智能体", "requirements-assessment", "评估工程师、研发负责人", "从成熟度、关键技术、物料、周期、测试和交付判断可行性", "可承接建议、前置条件、风险", "评估表 + 规则 + 证据推理", "P0"],
  [24, "行业标准与合规智能体", "requirements-assessment", "工程师、质量、商务", "按医疗、煤矿、轨交、3C等行业识别适用标准和验证要求", "标准清单、测试项、合规风险", "标准知识库 + 规则匹配", "P1"],
  [25, "研发人天估算智能体", "quote-finance", "评估工程师、商务", "按专业、难度、交付和历史项目估算人天", "岗位人天、难度系数、依据", "历史类比 + 规则计算", "P0"],
  [26, "BOM与样机成本智能体", "quote-finance", "工程师、采购、商务", "解析 BOM，计算样机物料、加工、装配和数量阶梯成本", "成本明细、替代料、区间", "BOM解析 + 价格接口 + 计算器", "P0"],
  [27, "外部询价与价格比较智能体", "quote-finance", "采购、商务", "从授权供应商和询价记录获取价格并比较交期、质量和税费", "价格对比、最低成本、风险", "供应商接口 + 归一化规则", "P1"],
  [28, "参考报价智能体", "quote-finance", "工程师、商务、审批人", "汇总人天、物料、管理费、税费和风险生成报价", "报价草稿、依据、区间", "确定性计算 + 大模型解释", "P0"],
  [29, "定价与毛利策略智能体", "quote-finance", "商务、管理者", "结合客户类型、预算、服务标准和风险提示价格与毛利策略", "建议价格、毛利区间、策略理由", "客户画像 + 规则 + 场景模拟", "P1"],
  [30, "报价预审与版本智能体", "quote-finance", "商务、审批人", "检查漏项、异常、审批条件并比较报价版本", "预审结果、版本差异、审批项", "规则校验 + 语义 Diff", "P0"],
  [31, "报价解释与谈判辅助智能体", "quote-finance", "商务、客户", "将工程成本、范围和风险转成客户可理解说明，辅助谈判", "报价说明、问答、谈判要点", "可控生成 + 报价证据", "P1"],
  [32, "合同生成智能体", "quote-finance", "商务、法务、PM", "生成 NDA、合同、技术附件、交付和验收清单草稿", "受控合同文档", "模板引擎 + 条款库", "P1"],
  [33, "合同一致性与风险智能体", "quote-finance", "商务、法务、PM", "检查需求、报价、合同、付款、交付和验收是否一致", "冲突、风险条款、修改建议", "字段比对 + 语义检查", "P0"],
  [34, "收款与支付助手", "quote-finance", "客户、商务、财务", "生成项目收款码/支付链接，关联订单和付款节点，提示到账", "支付请求、付款状态、凭证", "支付产品接口 + 订单规则", "P1"],
  [35, "对账与结算智能体", "quote-finance", "财务、商务、供应商", "匹配合同、订单、收付款、交付和结算数据，识别差异", "对账单、差异项、结算建议", "确定性匹配 + 异常检测", "P1"],
  [36, "发票与税务辅助智能体", "quote-finance", "财务、商务", "按主体、业务类型和税率准备开票信息并检查一致性", "开票申请、税率提示、异常", "财税规则 + 发票接口", "P2"],
  [37, "项目拆解智能体", "project-delivery", "PM、项目负责人", "将合同范围拆成 WBS、任务、依赖和里程碑", "项目计划、任务包、责任矩阵", "项目模板 + 任务图", "P1"],
  [38, "项目组组建智能体", "project-delivery", "PM、研发负责人", "按技能、负荷、项目经验和协作关系推荐成员", "项目组、角色、分工依据", "技能图谱 + 资源排序", "P1"],
  [39, "计划排程智能体", "project-delivery", "PM、项目负责人", "根据任务依赖、资源和关键物料形成计划并模拟延误", "甘特计划、关键路径、场景方案", "约束求解 + 排程算法", "P1"],
  [40, "AI项目助理", "project-delivery", "项目成员、客户", "维护项目上下文，回答状态、资料、责任和下一步", "项目问答、摘要、提醒", "项目 RAG + 权限上下文", "P1"],
  [41, "任务分派与自动化智能体", "project-delivery", "PM、项目成员", "按工作流自动创建 AI/人员任务、催办、升级和关闭", "任务、提醒、自动化记录", "工作流引擎 + 调度器", "P1"],
  [42, "会议行动跟踪智能体", "project-delivery", "项目成员、客户", "将会议结论转为任务、确认项、风险和需求变更", "行动项、责任人、变更建议", "ASR + 任务抽取", "P1"],
  [43, "项目风险预警智能体", "project-delivery", "PM、管理者", "监控延期、物料、成本、质量、范围和客户确认风险", "风险预警、影响、建议动作", "事件规则 + 异常检测", "P1"],
  [44, "需求变更影响智能体", "project-delivery", "客户、商务、PM、工程师", "评估变更对结构、电子、软件、采购、周期、成本和验收的影响", "变更影响单、附加合同建议", "依赖图 + 成本/排程重算", "P1"],
  [45, "执行前复核智能体", "project-delivery", "商务、PM、项目负责人", "检查需求、报价、合同、计划和验收的缺失与冲突", "复核清单、责任人、关闭记录", "规则 + 语义一致性检查", "P0"],
  [46, "产出物与验收管理智能体", "project-delivery", "PM、客户、质量", "管理协作资料、项目产出物、正式交付物、版本、评审和验收", "交付清单、版本状态、验收包", "版本模型 + 里程碑规则", "P1"],
  [47, "产品系统架构智能体", "engineering-rd", "产品经理、系统工程师", "将需求分成机械、电子、软件、通信、电源和交互架构", "系统框图、接口、关键决策", "架构模板 + RAG + 人工评审", "P1"],
  [48, "工业设计与概念图智能体", "engineering-rd", "工业设计师、客户", "生成外观方向、CMF、使用方式和概念图用于早期沟通", "概念图、设计方向、约束", "图像生成 + 设计 Brief", "P1"],
  [49, "机械结构方案智能体", "engineering-rd", "机械工程师", "推荐机构、材料、连接、传动、防护、装配和可制造性方向", "结构方案草稿、风险、待验证项", "机械知识库 + 方案检索", "P1"],
  [50, "电子系统方案智能体", "engineering-rd", "硬件工程师", "推荐主控、电源、通信、传感、接口和保护方案", "电子架构、器件方向、风险", "器件库 + 规则 + RAG", "P1"],
  [51, "器件与关键物料选型智能体", "engineering-rd", "硬件、采购", "按性能、成本、交期、生命周期、国产化和认证选型", "候选器件、对比、选型依据", "参数过滤 + 供应数据", "P1"],
  [52, "原理图审查智能体", "engineering-rd", "硬件工程师", "检查电源、接口、保护、器件连接、标注和常见设计问题", "审查清单、风险、定位", "EDA解析 + 规则库", "P2"],
  [53, "PCB与DFM审查智能体", "engineering-rd", "PCB、工艺工程师", "检查布局、布线、层叠、可制造性和工厂规则", "DFM问题、修改建议", "EDA/DFM工具接口 + 规则", "P2"],
  [54, "固件与软件开发智能体", "engineering-rd", "软件、嵌入式工程师", "辅助架构、代码、测试、接口和文档，关联项目上下文", "代码草稿、测试、说明", "编码模型 + 代码库 + CI", "P1"],
  [55, "工程BOM智能体", "engineering-rd", "硬件、结构、采购、工艺", "规范 EBOM、替代料、版本、生命周期和采购属性", "标准EBOM、替代关系、风险", "BOM图谱 + 器件接口", "P1"],
  [56, "模块复用智能体", "engineering-rd", "全体工程师", "检索历史结构、原理图、PCB、BOM、代码和测试模块", "候选模块、适配差异、风险", "多模态检索 + 知识图谱", "P1"],
  [57, "设计评审智能体", "engineering-rd", "项目负责人、研发负责人", "按专业 Checklist 预检设计文件和评审材料", "评审问题、证据、关闭状态", "规则 + 文件理解", "P1"],
  [58, "测试方案智能体", "engineering-rd", "测试、研发、质量", "根据需求、标准、风险和历史问题生成测试项", "测试计划、用例、判定标准", "标准库 + 风险映射", "P1"],
  [59, "工程文档生成智能体", "engineering-rd", "工程师、PM", "从设计和任务数据生成方案、说明、清单和报告草稿", "受控工程文档", "模板引擎 + 数据绑定", "P1"],
  [60, "跨专业一致性智能体", "engineering-rd", "项目负责人、系统工程师", "检查机械、电子、软件、BOM、测试和交付之间的接口与版本一致性", "冲突、接口缺口、责任人", "工程对象图谱 + 规则", "P2"],
  [61, "工厂与供应商匹配智能体", "manufacturing-quality", "采购、供应链、PM", "按工艺、设备、精度、认证、质量、数量和交期匹配资源", "候选工厂/供应商、评分", "能力画像 + 硬过滤 + 排序", "P1"],
  [62, "RFQ生成与询价智能体", "manufacturing-quality", "采购、供应链", "将需求、图纸、BOM、数量和交期转为标准询价包", "RFQ、询价清单、收件人", "模板 + 数据绑定 + 工作流", "P1"],
  [63, "报价归一与比价智能体", "manufacturing-quality", "采购、供应链", "将不同供应商报价按税费、交期、MOQ、质量和范围归一", "比价表、异常、推荐", "规则计算 + 语义归一", "P1"],
  [64, "供应商评分智能体", "manufacturing-quality", "采购、质量", "综合响应、价格、质量、交付、整改和合作记录评分", "供应商画像、风险、分层", "指标评分 + 历史结果", "P1"],
  [65, "采购审批建议智能体", "manufacturing-quality", "采购、审批人", "根据价格、供应风险、预算和交期形成采购建议", "审批摘要、建议、风险", "采购规则 + 证据解释", "P2"],
  [66, "物料齐套与替代智能体", "manufacturing-quality", "采购、工程师、计划", "监控缺料、长周期物料、替代料和齐套时间", "缺料清单、替代建议、齐套日期", "BOM图谱 + 库存/供应接口", "P1"],
  [67, "样机试制规划智能体", "manufacturing-quality", "项目负责人、采购、工艺", "形成样机数量、加工、PCBA、装配、测试和迭代计划", "试制计划、资源、风险", "项目模板 + 排程", "P1"],
  [68, "工艺路线智能体", "manufacturing-quality", "工艺工程师", "根据设计、材料、设备和数量推荐加工与装配路线", "工艺路线、设备、参数范围", "工艺知识库 + 规则", "P1"],
  [69, "作业指导智能体", "manufacturing-quality", "工艺、生产、测试", "将设计和工艺转为装配、烧录、测试和包装步骤", "SOP/作业指导草稿", "模板 + 多模态生成", "P1"],
  [70, "生产排程智能体", "manufacturing-quality", "生产计划、车间管理", "根据订单、物料、工位、设备和人员安排试制/生产", "排程、资源冲突、交付预测", "约束优化 + MES接口", "P2"],
  [71, "生产任务助手", "manufacturing-quality", "操作员、班组长", "推送任务、图纸、作业指导和注意事项，采集完成与异常", "工单状态、现场记录", "移动端/语音 + 工作流", "P2"],
  [72, "语音报工与记录智能体", "manufacturing-quality", "工程师、操作员、质检", "将现场语音、照片和视频转为结构化进度、质量和问题记录", "报工、异常、照片关联", "ASR/视觉 + 结构化抽取", "P1"],
  [73, "质量检验智能体", "manufacturing-quality", "质量、测试、工程师", "生成来料、过程、整机和出货检查项并辅助记录", "检验计划、结果、不合格项", "标准库 + Checklist + 视觉", "P1"],
  [74, "缺陷与根因分析智能体", "manufacturing-quality", "质量、研发、生产", "汇总缺陷、测试、工艺、批次和变更，辅助定位根因", "根因假设、验证路径、整改", "知识图谱 + 统计 + RAG", "P2"],
  [75, "设备维护智能体", "manufacturing-quality", "设备、生产管理", "监控保养、故障和运行记录，推荐维护计划", "保养任务、故障建议、备件", "设备数据 + 规则/预测", "P2"],
  [76, "制造成本复盘智能体", "manufacturing-quality", "财务、工艺、项目负责人", "汇总物料、加工、人工、返工、报废和交付成本", "实际成本、偏差、改进项", "成本数据模型 + 差异分析", "P2"],
  [77, "客户验收智能体", "delivery-knowledge", "客户、PM、质量", "按合同和交付清单组织验收、问题和签署记录", "验收包、问题、验收单", "合同数据绑定 + 工作流", "P1"],
  [78, "售后问题分诊智能体", "delivery-knowledge", "客服、客户、工程师", "识别问题类型、严重度、产品版本和责任路径", "工单分类、优先级、建议", "意图分类 + 产品知识库", "P2"],
  [79, "故障诊断智能体", "delivery-knowledge", "客服、研发、质量", "根据日志、图片、批次、配置和历史问题辅助诊断", "故障假设、排查步骤、证据", "RAG + 规则 + 工具诊断", "P2"],
  [80, "用户反馈与迭代智能体", "delivery-knowledge", "产品、研发、客户成功", "聚类售后、使用反馈和改进诉求，识别产品迭代机会", "反馈主题、优先级、迭代建议", "聚类 + 主题分析 + 业务评分", "P2"],
  [81, "项目归档智能体", "delivery-knowledge", "PM、工程师、质量", "自动归类需求、设计、BOM、测试、工艺、验收和复盘资料", "完整项目档案、缺失项", "文件分类 + 版本规则", "P1"],
  [82, "知识提取智能体", "delivery-knowledge", "领域工程师、AI运营", "从项目成功、失败、变更和异常中提取可复用知识", "知识条目、标签、来源", "RAG预处理 + 结构化抽取", "P1"],
  [83, "知识质量与复用智能体", "delivery-knowledge", "领域负责人、AI运营", "检查知识完整度、时效、冲突和使用效果，管理发布", "审核建议、过期项、复用数据", "规则 + 使用反馈分析", "P2"],
  [84, "工艺包组装智能体", "delivery-knowledge", "工艺、质量、项目负责人", "将正式设计、BOM、工艺、测试和质检组装为可复用工艺包", "工艺包、版本、适用范围", "资产模板 + 里程碑规则", "P1"],
  [85, "总控编排智能体", "ai-governance", "全平台", "根据任务、角色、上下文和状态选择工作流、智能体和工具", "执行计划、路由、降级", "Agent编排器 + 状态机", "P0"],
  [86, "工作流配置智能体", "ai-governance", "产品、业务运营、AI运营", "将业务流程描述转为节点、条件、任务、人工门和异常路径草稿", "工作流草稿、版本差异", "DSL/低代码 + 生成模型", "P1"],
  [87, "模型路由智能体", "ai-governance", "AI运营、系统", "按任务质量、时延、成本和数据等级选择模型与降级策略", "模型选择、调用记录", "路由规则 + 在线评测", "P1"],
  [88, "知识检索编排智能体", "ai-governance", "全部业务智能体", "根据任务决定检索哪些知识库、结构化库和外部来源", "证据包、来源、检索日志", "混合检索 + 重排", "P0"],
  [89, "记忆与上下文智能体", "ai-governance", "用户、项目成员", "管理用户、Case、项目和会话上下文，避免跨客户数据串用", "上下文摘要、记忆更新", "分层记忆 + 权限隔离", "P0"],
  [90, "权限与数据安全智能体", "ai-governance", "管理员、合规、系统", "检查模型、用户、智能体和工具对数据的访问权限", "允许/拒绝、脱敏、告警", "ABAC/RBAC + 数据分级", "P0"],
  [91, "数据质量智能体", "ai-governance", "数据运营、业务负责人", "识别缺失、重复、冲突、异常、错误标签和过期数据", "数据问题、责任人、修复建议", "规则 + 异常检测", "P1"],
  [92, "智能体评测智能体", "ai-governance", "AI运营、产品、领域专家", "自动运行案例集，对准确、完整、证据、时延和成本评分", "评测报告、回归问题", "Eval框架 + 黄金集", "P0"],
  [93, "证据与幻觉检查智能体", "ai-governance", "AI运营、业务用户", "检查输出是否有来源、是否超出证据、是否把推测当事实", "证据覆盖、风险标记", "引用校验 + 事实检查", "P0"],
  [94, "AI成本运营智能体", "ai-governance", "AI运营、财务、管理者", "监控模型、智能体、客户、项目的调用量、成本和收益", "成本看板、异常、优化建议", "Token/调用计量 + 成本规则", "P1"],
  [95, "审计与决策日志智能体", "ai-governance", "管理员、审计、管理者", "记录 AI 调用、数据访问、文件操作和人工决策", "审计日志、决策链、告警", "不可篡改日志 + 事件模型", "P0"],
  [96, "反馈学习智能体", "ai-governance", "产品、AI运营、领域专家", "汇总确认、驳回、修改、最终结果和复用效果，形成改进任务", "错误类型、训练/规则任务、版本建议", "反馈闭环 + 主动学习", "P1"],
];

const SUGGESTED_INPUTS: Record<AgentCapabilityDomainId, string[]> = {
  "innovation-service": ["已有想法、需求资料或产品背景", "目标用户、应用场景和主要约束"],
  "sales-operations": ["客户与联系人信息", "已有沟通、会议或线索资料"],
  "requirements-assessment": ["需求文档、图纸或附件", "范围、约束、交付期望和待确认问题"],
  "quote-finance": ["已确认的需求范围", "BOM、工时、合同或价格资料"],
  "project-delivery": ["项目范围、计划和当前状态", "任务、会议、风险或变更记录"],
  "engineering-rd": ["设计输入、图纸、BOM、代码或测试资料", "适用标准、接口和工程约束"],
  "manufacturing-quality": ["BOM、图纸、工艺或供应资料", "数量、交期、质量和现场记录"],
  "delivery-knowledge": ["合同、交付物、验收或售后资料", "项目版本、问题记录和复盘材料"],
  "ai-governance": ["目标流程、智能体或模型运行记录", "权限、质量、成本或审计约束"],
};

export const AGENT_CAPABILITIES: AgentCapability[] = ROWS.map(([
  id,
  name,
  domainId,
  audiences,
  coreCapability,
  outputs,
  implementation,
  priority,
]) => {
  const catalogId = getCatalogCapabilityId(id);
  return {
    id,
    catalogId,
    status: "catalog_only",
    capabilityPackId: catalogId,
    name,
    domainId,
    priority,
    audiences: [audiences],
    coreCapability,
    outputs: [outputs],
    implementation,
    suggestedInputs: [...SUGGESTED_INPUTS[domainId]],
  };
});

export function filterCapabilities(
  items: readonly AgentCapability[],
  filters: CapabilityFilters,
) {
  const query = filters.query.trim().toLocaleLowerCase();
  return items.filter((item) => {
    const searchable = [
      item.name,
      ...item.audiences,
      item.coreCapability,
      ...item.outputs,
    ].join(" ").toLocaleLowerCase();

    return (!query || searchable.includes(query))
      && (filters.domainId === "all" || item.domainId === filters.domainId)
      && (filters.priority === "all" || item.priority === filters.priority);
  });
}

export function buildCapabilityPrompt(capability: AgentCapability) {
  return [
    `我想使用「${capability.name}」完成一项任务。`,
    "",
    "业务目标：",
    "",
    "已有资料：",
    ...capability.suggestedInputs.map((item) => `- ${item}`),
    "",
    "需要协助：",
    capability.coreCapability,
    "",
    "期望输出：",
    ...capability.outputs.map((item) => `- ${item}`),
    "",
    "补充约束或人工确认点：",
  ].join("\n");
}
