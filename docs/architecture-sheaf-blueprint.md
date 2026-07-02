# 结构化 diff / merge / 版本控制 —— 统一架构蓝图

> 一份跨仓库（tate / mumford / etale / shtuka）的设计记录。核心是把 seq / grid /
> tree / table 统一成**一个本体**：带 identity 的树 = location → value 的 **sheaf**。
> diff = 两个 section 的差；merge = section 的 gluing；conflict = gluing 的障碍。

---

## 0. 理论本体（一句话）

**一切皆树。** 一个对象 = 一个 `Location → Value` 的映射，其中：

- **Location** = 从根到节点的路径，键是 **identity**（内在稳定标识），不是位置。
- **Value** = 该节点的标量内容 **＋ 结构位置（parent / 顺序）作为普通字段** ＋ 一个显式的 `⊥ (absent)` 状态。
- Location 集合带**前缀偏序**（父 < 子）→ 这就是一个 Grothendieck 拓扑（父被子覆盖）
  → 树数据自动满足 sheaf 的粘合公理 → 它是 **sheaf**，不只是 presheaf。

seq / grid / table 是这棵树的**退化深度**，不是四个平行对象：

| 对象 | Location 形状 | 键（identity）从哪来 |
|---|---|---|
| tree（JSON/XML） | `[root, node#id, attr]` | 节点 identity（现成） |
| table / dataframe | `[pk, column]`（深度 2） | 主键 × 列名（现成） |
| grid（无主键） | `[rowKey, colKey]`（深度 2） | 坐标；无键时坐标下降**造键**（脏，隔离） |
| seq（行） | `[lineId]`（深度 1） | LCS 造稳定行 id（脏，seq 固有难度） |

**区分只存在于"造键适配器"层，不存在于内核。** 内核只认树。

---

## 1. 三条承重墙（哪些数学是真承重，不是装饰）

1. **版本历史 = DAG**（历史层，和 Git/Pijul 一样，无争议）。
2. **对象 = location→value 的 sheaf**（数据层）：diff 是 section 差，规范性对
   tree/table 成立（identity/主键规范），对 seq 准规范（LCS），对无键 grid **不规范**
   → grid 坐标下降**明确排除在代数核心外**，退化成造键预处理。
3. **merge = sheaf gluing，冲突 = gluing 障碍**（合并层）：merge 是**全函数**（永远成功），
   结果可能是"带冲突态的对象"，冲突态可被后续 patch 解决（偷自 Pijul 的唯一思想）。

**放弃的（因为我们是 snapshot + Merkle，不是 delta）：** patch 作为第一性对象、
patch 可交换群/群胚。这些是 Pijul 的 delta 模型逼出来的，我们不需要，硬留是送把柄。

---

## 2. 压力测试：cover 不了 / 会硌手的地方（已知边界，接受降级）

| # | 问题 | 处理 |
|---|---|---|
| 1 | **有序层的并发结构编辑**（ours/theirs 在同位置并发插入不同行）。sheaf 对顺序是盲的：两个新 key 无序关系，glue 给不出先后。这是每个 order-sensitive 层都会复现的**唯一硬点**。 | **降级为 order-conflict，交给人。** 不去和 Pijul 拼"任意并发重排自动无冲突"那最难的 5%。这是主动取舍。 |
| 2 | **移动检测**（同 identity 换父）。若 location=path，会被误看成删+增。 | **identity 当 location，结构位置（parent/order）放进 value。** 移动 → value 变化 → 回到 sheaf 甜点区。 |
| 3 | **版本间 location 集合会变**（增删节点）。 | 底空间取两版并集，value 域提升 `⊥ (absent)`。absent 是语义明确的一等状态，**不要用裸 `Option` 糊**。 |
| 4 | **pdf 无稳定结构**。 | pdf 仅进 **diff 展示**（shtuka 现有功能），**不接版本内核**。诚实划界。 |
| 5 | **非局部语义约束**（外键、define.xml 的 ItemRef→ItemDef）。 | 出范围。内核只保证**结构**合并正确，语义校验是上层的事（和 Git 一样）。 |

**性能：无红旗。** Merkle 增量 O(n)、identity 匹配 hashmap O(n)、唯一贵的 LCS 本就在 tate。

---

## 3. Crate 图景（做减法：4 个，砍掉 topos）

依赖单向：**tate ← mumford ← etale ← shtuka**（一条链，不是网）。

```
┌──────────────────────────────────────────────────────────────┐
│ tate  (crates.io, 纯, 零重依赖)                                 │
│   • Tree 本体：Location / Value(含 ⊥) / sheaf                   │
│   • diff / apply / invert / compose / merge(gluing) / conflict  │
│   • serde → Tree  (feature = "serde"，免费上树，最通用入口)      │
└──────────────────────────────────────────────────────────────┘
        ▲                                    可序列化对象走这条↑，
        │ tate                               根本不碰 mumford
┌───────┴──────────────────────────────────────────────────────┐
│ mumford  (crates.io) = 重依赖隔离墙 + 格式专属 keying           │
│   ⚠ 价值是"把 pdfium/calamine/zip 关在墙内"，不是原创算法        │
│   feature-gated：                                               │
│     excel  → calamine                                           │
│     docx/pptx/xml → quick-xml + zip                             │
│     pdf    → pdfium   (最重，单独可选)                           │
│     folder → walkdir + sha2                                     │
│   每种格式：字节 → 解析(现成库) → 你的 Tree(薄映射)              │
└──────────────────────────────────────────────────────────────┘
        ▲
        │ tate (必需) + mumford (可选, 要格式支持时才开)
┌───────┴──────────────────────────────────────────────────────┐
│ etale  (你的, j-yang) = 版本内核库 + 薄 CLI/git-driver bin      │
│   • Merkle 内容寻址存储 + 结构共享(未变子树共享 → delta 免费)    │
│   • 快照 + 版本 DAG + 分支                                       │
│   • Schnorr 签名 + Merkle proof (密码学入口)                    │
│   • bin: CLI + git custom diff/merge driver                     │
│   （吸收了原计划的 topos，不单独建 crate）                       │
└──────────────────────────────────────────────────────────────┘
        ▲
        │ etale + mumford + tate
┌───────┴──────────────────────────────────────────────────────┐
│ shtuka  (公司财产, 独立 app)                                    │
│   • shtuka-core：CDISC/define.xml enrichment、RTF、临床 dispatch │
│   • track 功能改由 etale 驱动（手写快照上移，泛化成内核特例）    │
│   • UI 接口不变（拿到的仍是 per-cell/per-node diff 结果）        │
└──────────────────────────────────────────────────────────────┘

未来第 5 个：motive (ZK)。只在前四阶段扎实后启动，现在不碰。
```

### 关键 crate 决定与理由

- **tate / mumford 不合并**：tate 必须纯（零重依赖），mumford 扛 pdfium/calamine。
  分开是依赖卫生，当初就拆对了。
- **serde→Tree 放 tate（feature），不放 mumford**：它零重依赖、最通用。于是
  "可序列化对象做版本控制" = `serde → tate → etale`，**完全不碰 mumford**，极干净。
- **mumford 改 feature-gated 适配器集合**：按格式付费，pdf(pdfium) 单独可选。它的
  "局限"通过 feature gate 变成"精确可选性"，是优点。价值重定性为**隔离墙**。
- **不建 topos**：版本内核折进 etale。sheaf 理论概念上活在 tate（它是代数），
  不需要单独 crate 装范畴论。兑现"不让六个名字摊薄"。
- **etale 从 CLI 升级成 库 + bin**：同事要 `cargo add etale`，不是命令行门面。

---

## 4. 分阶段实现计划（每阶段有可演示产出）

> **进度（tate 0.4 / mumford 0.5，2026-07）：Phase 0 ✅、Phase 1 ✅ 已完成；
> Phase 2 部分完成（grid 折叠已实证，keying 尚未上移 mumford）。**

### Phase 0 — 在 tate 确立统一 `Tree` 本体 ✅ 已完成
把 `Tree = Location(identity 键) → Value(含结构位置、含 ⊥)` 定为 tate 一等类型。
grid/lines 降级成"产出 Tree 的 keying 适配器"。**主要是整理现有代码，非重写。**
产出：统一类型 + 现有 diff 跑在其上。

**落地记录：** tate 新增 `section` 模块，`Section = BTreeMap<Location, Value>`
成为一等公开类型（`TreeNode::to_section` / `Section::to_tree`）。identity 是
location，结构位置（order）与标量内容是 value —— 移动/改名 = value 变化。

### Phase 1 — 补齐/修正代数核心 ✅ 已完成
1. 修 `tree_merge`：不再静默吞冲突。四类 gluing 障碍（`Attr` / `Text` /
   `AddAdd` / `ModifyDelete`）记录为 `TreeConflict`。**顺带发现并修复 text
   变化在 merge 时被丢弃的真 bug**（JSON 标量 / grid cell 首当其冲）。
2. 实现 `apply / invert / compose`（`patch.rs`，无损，跑在 `Section` 上）。
3. **proptest 验证定律**（`tests/patch_laws.rs`，各 2000 例）：diff/apply 互逆、
   identity、invert、compose = 顺序 apply、结合律、逆元抵消、merge pushout 对称。
产出：定律有随机测试保障的补丁代数。

### Phase 2 — keying 适配器 + 有序层冲突降级 ◐ 部分完成
形式化"造键"：tree=identity、table=pk×col、grid=坐标(无键 fallback 坐标下降)、
seq=LCS 稳定 id。实现 §2.1 的**有序层并发插入 → order-conflict** 降级。
产出：mumford 每格式产出统一 Tree；docx/pdf 顺带获得结构化 diff（现在没有）。

**已完成：** 用探针（`tests/grid_stable_key_probe.rs`）实证"grid → 用坐标下降
对齐结果造稳定键的 Section → tree_merge → grid"逐字节复现旧 `grid_merge`
（不相交编辑 / 同格冲突 / 行插入移位）。据此 **tate 0.4 收敛为单一 merge**：
删除 `grid_merge` 与 line-level `merge`，只留 `tree_merge`；LCS / 坐标下降作为
不可归约的造键算法保留（并各自产出展示结果，如 `GridDiff`）。
**尚未完成：** 把 excel→tree / text→tree 的造键编排从"调用点"正式上移到 mumford
适配器；docx/pdf 结构化 diff；有序层 order-conflict 降级。

### Phase 3 — etale 版本内核
Merkle 内容寻址存储 + 结构共享（delta 免费）+ 快照 + 版本 DAG。shtuka 的 track
逻辑上移泛化。加 Schnorr 签名 + Merkle proof。
产出：可嵌入的版本内核库 + CLI + git diff/merge driver。

### Phase 4 — shtuka 重新接线
shtuka-core 的 track 改用 etale；临床适配保留；app 行为不变。
产出：理论第一次在真实产品里跑。

### Phase 5（未来）— motive (ZK)
"验证见证而非重算"框架：tate 生成 changeset 作为私有输入，电路只验证
"见证合法且代价 ≤ 阈值 / 变更范围 ⊆ 允许集"。先做"变更范围证明"一个原语。
只在前四阶段扎实后启动。

---

## 5. 命名（代数几何体系，保留但配人话副标题）

| crate | 人话副标题（README 第一行必须有） |
|---|---|
| tate | structured diff & patch algebra |
| mumford | format → tree adapters |
| etale | structured version-control kernel |
| shtuka | clinical document diff app |
| motive | zero-knowledge proofs over structured changes（未来） |

品牌辨识度保留，可理解性不牺牲。
