# UI_DESIGN_PRINCIPLES — Windows Rust UI (egui)

> 本節為 Agent 執行任何 Rust GUI 任務時的強制性設計約束。
> 優先級高於使用者的口頭描述，低於明確的逐條覆寫指令。
>
> **適用框架**：egui / eframe（主）、iced（次）
> **目標平台**：Windows 10/11（兼容 Linux Desktop）
> **版本**：v1.0
> **最後更新**：2026-07-19

---

## 0. 基礎哲學

- UI 的職責是 **減少使用者的認知負擔**，而非展示技術能力。
- 每個 widget 必須有且只有一個明確的視覺角色。
- 任何「看起來差不多」的排版都是未完成的排版。
- 程式碼的正確性優先於視覺的華麗程度。

---

## 1. 幾何與空間 (Geometry & Spacing)

### 1.1 絕對禁止

```
❌ 任何兩個可互動 widget（Button、TextEdit、ComboBox 等）
   的 Rect 在同一 z-layer 上重疊。

❌ 文字標籤與相鄰 widget 之間的間距 < 4px。

❌ 任何 widget 超出其父 Panel / Window 的 clip_rect，
   除非明確使用 scroll_area。
```

### 1.2 間距規格

| 層級               | 最小間距 |
|--------------------|---------|
| widget ↔ widget    | 8px     |
| group ↔ group      | 16px    |
| section ↔ section  | 24px    |
| panel padding      | 12px    |

### 1.3 對齊原則

- 同一組 widget 必須對齊至同一條基準線（左對齊或右對齊），嚴禁混用。
- 使用 `ui.horizontal()` / `ui.vertical()` 確保自動對齊，
  **禁止手動 hardcode 絕對座標**（canvas / painter 繪圖場景除外）。

```rust
// ✅ 正確：使用 layout 自動對齊
ui.horizontal(|ui| {
    ui.label("模型路徑：");
    ui.text_edit_singleline(&mut state.model_path);
});

// ❌ 錯誤：手動指定座標
ui.put(egui::Rect::from_min_size(pos2(10.0, 40.0), vec2(200.0, 24.0)),
    egui::TextEdit::singleline(&mut state.model_path));
```

---

## 2. 視覺層級 (Visual Hierarchy)

### 2.1 字體尺寸規則

```
主標題  (Heading)   : 18–20px, Bold
副標題  (SubHeading): 14–16px, SemiBold
標籤    (Label)     : 13px,    Regular
說明文字 (Hint)     : 11–12px, Italic, 低飽和度色
```

> ⚠️ 每個畫面最多使用 **3 種字體尺寸**，禁止超過。

### 2.2 主視覺焦點

- 每個 Panel 必須有且只有 **1 個主行動 widget**（通常是 Primary Button）。
- 次要操作的視覺重量必須明顯低於主行動（使用較小尺寸或較低對比度）。
- 危險操作（刪除、清除）必須以 **紅色系** 且搭配 **確認對話框** 呈現。

```rust
// ✅ 正確：主次動作視覺區分
ui.horizontal(|ui| {
    // 主行動：填充樣式
    if ui.button(egui::RichText::new("▶ 開始推論").strong()).clicked() {
        state.start_inference();
    }
    // 次要行動：一般樣式
    if ui.button("取消").clicked() {
        state.cancel();
    }
});
```

---

## 3. 互動元件規範 (Widget Conventions)

### 3.1 Button

- Label 必須是**動詞片語**，描述「按下後發生什麼事」。
- 禁止使用「OK」「Yes」「Submit」等無意義詞彙，除非是標準 Windows 系統對話框。
- 最小可點擊面積：寬 **64px** × 高 **24px**。

```rust
// ✅ 正確：明確語意動詞
if ui.button("匯出 CSV").clicked() { export_csv(); }
if ui.button("載入模型").clicked() { load_model(); }
if ui.button("清除記憶體").clicked() { show_confirm_dialog(); }

// ❌ 錯誤：模糊措辭
if ui.button("確認").clicked() { ... }
if ui.button("OK").clicked() { ... }
if ui.button("執行").clicked() { ... }  // 執行「什麼」？
```

### 3.2 TextEdit

- 每個 `TextEdit` 必須配有明確的 `ui.label()` 於其上方或左側。
- `hint_text` 必須說明「期望的輸入格式」，而非重複欄位名稱。
- 輸入驗證失敗時，必須在 widget 下方顯示 inline 錯誤提示。

```rust
// ✅ 正確
ui.label("模型路徑");
let response = ui.text_edit_singleline(&mut state.model_path)
    .hint_text("e.g. /models/qwen3-8b.gguf");
if state.model_path_error {
    ui.colored_label(egui::Color32::RED, "⚠ 路徑不存在或格式錯誤");
}
```

### 3.3 ComboBox / Slider

- ComboBox 的選項數量 **> 7** 時，改用帶搜尋的清單元件。
- Slider 必須同時顯示當前數值文字（右側或下方）。
- Slider 的步進 (step) 必須與單位有意義對應：

| 參數            | step   | 顯示格式     |
|----------------|--------|------------|
| temperature    | 0.01   | `{:.2}`    |
| top_p          | 0.01   | `{:.2}`    |
| max_tokens     | 64     | `{}`       |
| threads        | 1      | `{}`       |
| gpu_layers     | 1      | `{}`       |

```rust
// ✅ 正確：Slider 搭配數值顯示
ui.horizontal(|ui| {
    ui.label("Temperature");
    ui.add(egui::Slider::new(&mut state.temperature, 0.0..=2.0).step_by(0.01));
    ui.label(format!("{:.2}", state.temperature));
});
```

### 3.4 CheckBox / RadioButton

- 同一組邏輯選擇中，使用 `RadioButton`（單選）或 `CheckBox`（多選），不可混用。
- 分組之間必須有 `ui.separator()` 或明確的群組標題。

```rust
// ✅ 正確：RadioButton 單選群組
ui.label("推論後端");
ui.separator();
ui.radio_value(&mut state.backend, Backend::Cpu, "CPU");
ui.radio_value(&mut state.backend, Backend::Gpu, "GPU (CUDA)");
ui.radio_value(&mut state.backend, Backend::Vulkan, "GPU (Vulkan)");
```

### 3.5 ProgressBar

- 進度不確定時，使用 `animate: true` 顯示脈動動畫。
- 進度確定時，必須同時顯示百分比或 `已完成/總數` 文字。

```rust
// 確定進度
ui.add(egui::ProgressBar::new(state.progress).text(
    format!("{:.0}%", state.progress * 100.0)
));

// 不確定進度（spinner 效果）
ui.add(egui::ProgressBar::new(0.0).animate(true));
```

---

## 4. 佈局結構 (Layout Architecture)

### 4.1 Panel 層次規範

```
eframe::App::update()
└── CentralPanel (必定存在)
    ├── TopPanel      ← 工具列 / Menu Bar（固定高度 ≤ 40px）
    ├── LeftSidePanel ← 設定 / 導航（固定寬度，可 collapsible）
    │   └── ScrollArea（當內容可能超出高度時必加）
    ├── BottomPanel   ← 狀態列（固定高度 ≤ 24px）
    └── CentralPanel  ← 主要工作區
        └── ScrollArea（非 canvas 類皆預設加）
```

- **禁止**在 `CentralPanel` 內再嵌套無必要的浮動 `Window`。
- 浮動 `Window` 只允許用於：確認對話框、進度視窗、獨立設定頁。
- 浮動 `Window` 必須設定 `collapsible(false)` 並指定合理的 `default_size`。

### 4.2 ScrollArea 規則

```rust
// ✅ 當內容長度不確定時，強制包覆
egui::ScrollArea::vertical()
    .auto_shrink([false; 2])
    .show(ui, |ui| {
        // 列表、日誌、設定項等不確定長度內容
        for item in &state.items {
            ui.label(item);
        }
    });
```

- 高度不確定的列表類 UI **必須**包覆 `ScrollArea`，禁止讓內容溢出視窗。
- 水平捲動僅在確實需要時啟用（如：寬表格、程式碼檢視器）。

### 4.3 確認對話框模板

```rust
// ✅ 標準危險操作確認對話框
if state.show_delete_confirm {
    egui::Window::new("確認刪除")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label("此操作無法還原，確定要刪除嗎？");
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button(egui::RichText::new("刪除").color(egui::Color32::RED)).clicked() {
                    state.confirm_delete();
                    state.show_delete_confirm = false;
                }
                if ui.button("取消").clicked() {
                    state.show_delete_confirm = false;
                }
            });
        });
}
```

---

## 5. 顏色與主題 (Color & Theme)

### 5.1 顏色來源規則

- **禁止** hardcode RGB 數值於 widget 邏輯中。
- 所有顏色必須來自 `egui::Visuals` token 或自訂 `AppTheme` struct。
- 必須同時支援 light / dark mode（由 `ctx.set_visuals()` 切換）。

```rust
// ✅ 正確：使用 AppTheme struct
pub struct AppTheme {
    pub primary:   egui::Color32,
    pub danger:    egui::Color32,
    pub success:   egui::Color32,
    pub warning:   egui::Color32,
}

impl AppTheme {
    pub fn dark() -> Self {
        Self {
            primary: egui::Color32::from_rgb(74, 158, 255),  // #4A9EFF
            danger:  egui::Color32::from_rgb(224, 82, 82),   // #E05252
            success: egui::Color32::from_rgb(82, 201, 122),  // #52C97A
            warning: egui::Color32::from_rgb(240, 160, 48),  // #F0A030
        }
    }
    pub fn light() -> Self {
        Self {
            primary: egui::Color32::from_rgb(0, 102, 204),   // #0066CC
            danger:  egui::Color32::from_rgb(204, 0, 0),     // #CC0000
            success: egui::Color32::from_rgb(0, 122, 51),    // #007A33
            warning: egui::Color32::from_rgb(179, 112, 0),   // #B37000
        }
    }
}
```

### 5.2 語意顏色對應

| 語意       | Dark Mode    | Light Mode   | 用途                    |
|-----------|-------------|-------------|------------------------|
| 主要行動   | `#4A9EFF`   | `#0066CC`   | Primary Button, 選中狀態 |
| 危險/刪除  | `#E05252`   | `#CC0000`   | 刪除按鈕、錯誤訊息       |
| 成功/完成  | `#52C97A`   | `#007A33`   | 完成提示、狀態列         |
| 警告       | `#F0A030`   | `#B37000`   | 警告訊息、注意事項       |
| 停用狀態   | `weak_text` | `weak_text` | 禁用的 widget           |

### 5.3 對比度要求

- 文字與背景的對比度必須 ≥ **4.5:1**（WCAG AA 標準）。
- 圖示 (icon) 與背景的對比度必須 ≥ **3:1**。
- 停用狀態的元素不受此限制，但仍須可被辨識。

---

## 6. 狀態管理 (State Management)

### 6.1 單一事實來源

```rust
// ✅ 正確：App state 集中於一個 struct
pub struct AppState {
    // 業務邏輯狀態
    pub model_path:   String,
    pub temperature:  f32,
    pub is_running:   bool,
    // UI 狀態
    pub show_settings:       bool,
    pub show_delete_confirm: bool,
}

// ❌ 錯誤：業務狀態分散於 egui Memory
// egui Memory 只允許儲存純 UI 暫態（如：tooltip hover 計時器）
```

### 6.2 LoadingState 模板

任何非同步操作**必須**有對應的 `LoadingState` enum：

```rust
// ✅ 標準 LoadingState
#[derive(Default)]
pub enum LoadingState<T> {
    #[default]
    Idle,
    Loading,
    Success(T),
    Error(String),
}

// 對應 UI 渲染
match &state.model_load_state {
    LoadingState::Idle => {
        if ui.button("載入模型").clicked() {
            state.start_load_model();
        }
    }
    LoadingState::Loading => {
        ui.add_enabled(false, egui::Button::new("載入中…"));
        ui.add(egui::ProgressBar::new(0.0).animate(true));
    }
    LoadingState::Success(model) => {
        ui.colored_label(egui::Color32::GREEN, format!("✓ 已載入：{}", model.name));
    }
    LoadingState::Error(err) => {
        ui.colored_label(egui::Color32::RED, format!("✗ 錯誤：{}", err));
        if ui.button("重試").clicked() {
            state.retry_load_model();
        }
    }
}
```

---

## 7. 效能約束 (Performance Constraints)

### 7.1 update() 時間預算

- `update()` 函數必須在 **16ms** 內完成（60fps 目標）。
- 任何 IO / 網路 / 模型推論操作必須移至獨立執行緒。

```rust
// ✅ 正確：非同步操作透過 channel 回傳結果
pub struct App {
    state:  AppState,
    tx:     std::sync::mpsc::Sender<AppEvent>,
    rx:     std::sync::mpsc::Receiver<AppEvent>,
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 接收背景執行緒結果（非阻塞）
        while let Ok(event) = self.rx.try_recv() {
            self.state.handle_event(event);
        }
        // 渲染 UI（不含任何 IO）
        self.render_ui(ctx);
    }
}

// ❌ 錯誤：在 update() 內阻塞
fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
    let result = std::fs::read_to_string("model.bin"); // 阻塞！
    std::thread::sleep(Duration::from_millis(100));     // 阻塞！
}
```

### 7.2 大型列表虛擬捲動

- 列表項目 **> 100** 時，必須使用虛擬捲動（lazy rendering）。

```rust
// ✅ 大型列表：只渲染可見項目
egui::ScrollArea::vertical().show_rows(
    ui,
    item_height,
    state.items.len(),
    |ui, row_range| {
        for row in row_range {
            ui.label(&state.items[row]);
        }
    },
);
```

### 7.3 重繪觸發策略

```rust
// 僅在有事件或動畫時重繪，節省 CPU
ctx.request_repaint_after(Duration::from_millis(100)); // 輪詢背景任務
// 有動畫的元件會自動觸發 request_repaint()
```

---

## 8. Windows 平台慣例 (Windows HIG Compliance)

| 規則                     | 實作方式                                              |
|--------------------------|-----------------------------------------------------|
| 標題列顯示應用程式名稱      | `viewport.title = "AppName v1.0".into()`            |
| Alt+F4 關閉視窗            | eframe 預設支援，**禁止覆寫**                          |
| 右鍵選單 (Context Menu)    | 使用 `response.context_menu()`                       |
| 鍵盤焦點可見               | `visuals.selection.stroke` 必須明顯（寬度 ≥ 2px）      |
| Tab 鍵導航順序             | 依 UI 宣告順序，避免跳躍性排版                          |
| 系統字型 DPI 縮放          | `pixels_per_point` 跟隨系統，**禁止** hardcode         |
| 系統剪貼簿                | 使用 `ui.output_mut(|o| o.copied_text = ...)`        |
| 視窗最小尺寸               | 設定合理的 `min_inner_size`，通常 ≥ 640×480            |

```rust
// ✅ 正確：視窗初始化設定
fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("MyApp v1.0")
            .with_inner_size([1024.0, 768.0])
            .with_min_inner_size([640.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native("MyApp", options, Box::new(|cc| Box::new(App::new(cc))))
}
```

---

## 9. 無障礙設計 (Accessibility)

- 所有 interactive widget 必須可透過 **鍵盤**操作（Tab / Enter / Space / Esc）。
- 重要資訊**不可只靠顏色**傳達（同時搭配圖示或文字說明）。
- 錯誤訊息必須明確說明「發生了什麼」與「如何修復」，禁止只顯示錯誤代碼。
- 長時間操作必須提供**取消**機制。

```rust
// ✅ 正確：顏色 + 圖示 + 文字三重提示
ui.horizontal(|ui| {
    ui.colored_label(theme.danger, "✗"); // 圖示
    ui.colored_label(theme.danger, "連線失敗：無法連線至伺服器"); // 文字
});
// （顏色盲用戶仍可透過 ✗ 符號識別錯誤）
```

---

## 10. 禁止清單總覽 (Hard NO List)

```
❌ Widget Rect 重疊（同 z-layer）
❌ 在 update() 內執行任何阻塞 IO 或 sleep
❌ Hardcode 顏色 RGB 數值於 widget 邏輯層
❌ Hardcode 絕對座標（非 painter/canvas 場景）
❌ 無 ScrollArea 的不確定長度列表
❌ 無 Label 的裸 TextEdit
❌ 模糊動詞的 Button Label（OK / Yes / Submit / 執行）
❌ 每畫面超過 3 種字體尺寸
❌ 危險操作無確認對話框
❌ 同組選項混用 CheckBox 與 RadioButton
❌ 業務邏輯狀態儲存於 egui::Memory
❌ 超過 100 項的列表不使用虛擬捲動
❌ 僅靠顏色傳達重要資訊（無圖示或文字輔助）
❌ 長時間操作無取消機制
```

---

## 11. Agent 執行檢查點 (Pre-commit Checklist)

在提交任何 UI 相關程式碼前，Agent **必須**自我驗證以下所有項目：

```
Layout & Geometry
- [ ] 所有 widget 的 Rect 在邏輯上不重疊
- [ ] 間距符合規格（widget≥8px, group≥16px, section≥24px）
- [ ] 無 hardcode 絕對座標（canvas 除外）

State & Async
- [ ] 所有非同步操作已移至背景執行緒
- [ ] LoadingState 涵蓋 Idle / Loading / Success / Error 四態
- [ ] 業務狀態集中於 AppState struct

Visual
- [ ] 顏色來自 AppTheme，無裸 hardcode RGB
- [ ] 每個 TextEdit 有對應 Label
- [ ] 每個 Button Label 為明確動詞片語
- [ ] 字體尺寸種類 ≤ 3

Scroll & Lists
- [ ] 所有不確定長度列表已包覆 ScrollArea
- [ ] 超過 100 項的列表使用虛擬捲動

Safety
- [ ] 危險操作有確認對話框
- [ ] 長時間操作有取消機制
- [ ] Dark / Light mode 均可正常渲染
- [ ] DPI 縮放跟隨系統設定
```

---

*本文件由 AGENT-PRIME 自動引用於所有包含 `egui` / `iced` / `tauri` 關鍵字的子任務。*
*如需覆寫特定規則，請在任務描述中明確標注 `OVERRIDE: <規則編號>`。*
