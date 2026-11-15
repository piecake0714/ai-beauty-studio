# AI Beauty Studio

Windows 桌面相片編輯軟體（開發中）。Tauri 2 + React/TypeScript 介面、Rust 核心、C++ 影像引擎，AI 於本機執行。

- 目前範圍：Windows 10 22H2 / Windows 11（x64），不對外販售。
- 功能狀態請見 [docs/STATUS.md](docs/STATUS.md)。只有在 CI 測試通過後，功能才會標示為「通過」。
- 第三方授權請見 [docs/THIRD_PARTY_LICENSES.md](docs/THIRD_PARTY_LICENSES.md)。

## 目前能做什麼

- 首頁：開啟影像、最近開啟的檔案（含縮圖）。
- 編輯器：以完整解析度檢視影像，滾輪縮放（5%–800%）、拖曳平移，顯示影像資訊。
- 調整（非破壞性，原始檔不會被修改）：曝光、對比、亮部、陰影、白色、黑色、亮度、色溫、色調、自然飽和度、飽和度、HSL 八色區；初學／進階模式；復原／重做；前後對照（分割檢視、按住 `\` 看原圖）。
- AI 執行環境：ONNX Runtime + DirectML（GPU），不可用時改用 CPU；「設定 → AI 與硬體」可檢視顯示卡並測試。
- 人像：開啟影像時自動偵測臉部（YuNet），磨皮滑桿即時作用於臉部膚色；工具列「一鍵增強」自動調整曝光、對比與色彩（一步可復原）。
- 背景：AI 分離主體與背景（BiRefNet lite）後，可去背、換純色或圖片、模糊背景、調整背景亮度與飽和度。
- 打光：自然光、柔光罩、漫射光、反光板，在畫面上拖曳光源；以 AI 估計的立體形狀計算受光面與背光面（不含投射陰影）。
- 檔案：儲存為 .aibs 專案（原圖只參照不複製，含復原紀錄與 AI 結果），匯出 JPG／PNG（PNG 可保留透明），意外關閉可自動還原。
- 模型管理（1.1）：設定中可檢視每個 AI 模型的版本、授權與狀態，重新驗證檔案、釋放記憶體；專案記錄 AI 結果的模型版本。
- 圖層（1.2）：調整圖層與影像圖層（最多 16 層），可調不透明度、8 種混合模式、排序、顯示／隱藏；圖層遮罩可從全部、無、主體或背景開始，用筆刷與橡皮擦（大小、硬度、不透明度）局部修改。
- 裁切（1.2）：自由或固定長寬比（原始、1:1、4:3、3:2、16:9），拖曳邊角與移動，匯出只輸出裁切範圍；可復原。
- 人像修飾（1.3）：去除瑕疵、淡化黑眼圈、亮眼、牙齒美白（依偵測到的臉部特徵點，只改顏色不改形狀）；修復筆刷可點選痘痘、斑點或雜物，用附近相似紋理取代；去背後可用「髮絲邊緣精修」讓邊緣貼合頭髮等細節。
- 設定：主題、語言、關於與第三方授權聲明。

## 結構

| 路徑 | 內容 |
| --- | --- |
| `apps/desktop/` | Tauri 應用程式；`src/` 是介面，`src-tauri/` 是主程式 |
| `crates/application-core/` | Rust 核心：檔案讀取、錯誤分類、傳給介面的畫面格式 |
| `crates/engine-sys/` | Rust 對 C++ 引擎的唯一入口 |
| `engine/` | C++ 引擎；`include/aibs/engine.h` 是公開的 C ABI |
| `tests/fixtures/` | 測試影像（自行產生的漸層圖，無版權問題） |
| `docs/` | 狀態表與授權清冊 |

## 取得安裝程式

每次推送後，GitHub Actions 會在 Windows 上建置並測試。到儲存庫的 **Actions** 頁面，點最新一次成功的執行，在 **Artifacts** 下載 `ai-beauty-studio-windows-installer`。

安裝程式沒有簽章，第一次執行時 Windows SmartScreen 會顯示警告，請選「其他資訊」→「仍要執行」。

## 在自己的 Windows 電腦建置

需要：Visual Studio 2022（含 C++ 桌面開發）、CMake、vcpkg、Rust、Node.js 22。

```powershell
# 0. 下載第三方模型（不存在 Git 中，會驗證雜湊）
python scripts/fetch_models.py

# 1. 建置並安裝 C++ 引擎到 engine/dist
cmake -S engine -B build/engine `
  -DCMAKE_TOOLCHAIN_FILE="$env:VCPKG_ROOT/scripts/buildsystems/vcpkg.cmake" `
  -DVCPKG_TARGET_TRIPLET=x64-windows-static-md `
  -DCMAKE_INSTALL_PREFIX="$PWD/engine/dist"
cmake --build build/engine --config Release
ctest --test-dir build/engine -C Release --output-on-failure
cmake --install build/engine --config Release

# 2. 前端
cd apps/desktop
npm install
npm test
cd ../..

# 3. 產生第三方授權聲明（安裝程式與 cargo test 都需要這個檔案）
cargo install cargo-about
cargo about generate --format json -c about.toml `
  --manifest-path apps/desktop/src-tauri/Cargo.toml -o build/cargo-about.json
python scripts/build_notices.py --cargo-about build/cargo-about.json `
  --vcpkg-share build/engine/vcpkg_installed/x64-windows-static-md/share `
  --node-modules apps/desktop/node_modules --npm-lock apps/desktop/package-lock.json `
  --out apps/desktop/src-tauri/resources/THIRD_PARTY_NOTICES.txt

# 4. Rust 測試與應用程式
cargo test --workspace
npx tauri dev      # 開發模式
npx tauri build    # 產生安裝程式
```
