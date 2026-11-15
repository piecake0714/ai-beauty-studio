# AI Beauty Studio

Windows 桌面相片編輯軟體（開發中）。Tauri 2 + React/TypeScript 介面、Rust 核心、C++ 影像引擎，AI 於本機執行。

- 目前範圍：Windows 10 22H2 / Windows 11（x64），不對外販售。
- 功能狀態請見 [docs/STATUS.md](docs/STATUS.md)。只有在 CI 測試通過後，功能才會標示為「通過」。
- 第三方授權請見 [docs/THIRD_PARTY_LICENSES.md](docs/THIRD_PARTY_LICENSES.md)。

## 目前能做什麼

- 首頁：開啟影像、最近開啟的檔案（含縮圖）。
- 編輯器：以完整解析度檢視影像，滾輪縮放（5%–800%）、拖曳平移，顯示影像資訊。尚無任何編輯功能。
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
