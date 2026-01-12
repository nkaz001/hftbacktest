# Task: 策略研究员实盘上手中文文档

## Goal
为策略研究员编写一份完整的中文文档，涵盖从 Python 策略到 Rust 实现再到 Docker 实盘部署的全流程。

## Current Phase
Phase 1

## Phases

### Phase 1: 研究现有代码结构
- [ ] 分析 strategies/ 目录结构 (Python 和 Rust 策略)
- [ ] 分析 factor-engine/ 因子引擎代码
- [ ] 分析 deploy/docker/ 部署配置
- [ ] 了解 connector 配置和命令行参数
- **Status:** in_progress

### Phase 2: 编写文档框架
- [ ] 创建文档目录结构
- [ ] 编写 Part 1: Python → Rust 策略迁移
- [ ] 编写 Part 2: Factor Engine 因子开发
- [ ] 编写 Part 3: Docker 实盘部署
- **Status:** pending

### Phase 3: 实际测试 - 新 Symbol 策略部署
- [ ] 选择一个新的 symbol (如 ETHUSDT)
- [ ] 从零配置并启动
- [ ] 记录每个步骤的实际输出
- [ ] 用真实内容填充文档
- **Status:** pending

### Phase 4: 文档完善与审核
- [ ] 检查文档完整性
- [ ] 添加常见问题解答
- [ ] 最终交付
- **Status:** pending

## 文档结构规划

```
docs/
├── STRATEGY_RESEARCHER_GUIDE.md          # 主文档入口
├── guides/
│   ├── 01-rust-basics.md                 # Rust 基础知识
│   ├── 02-python-to-rust.md              # Python → Rust 策略迁移
│   ├── 03-factor-engine.md               # Factor Engine 使用指南
│   ├── 04-docker-deployment.md           # Docker 实盘部署
│   └── 05-exchange-configs.md            # 交易所配置参考
```

## Decisions Made
| Decision | Rationale |
|----------|-----------|

## Errors Encountered
| Error | Attempt | Resolution |
|-------|---------|------------|

## Files to Examine
- strategies/obi_mm/ - Python OBI MM 策略
- strategies/obi_mm_rust/ - Rust OBI MM 策略
- factor-engine/src/ - 因子引擎实现
- deploy/docker/ - Docker 部署配置
- connector/src/ - Connector 实现和配置
