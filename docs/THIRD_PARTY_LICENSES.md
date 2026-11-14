# 第三方授權清冊

本專案不對外販售。每一列的「授權」是依上游專案公布內容所做的紀錄；「已核對」一欄只有在對照 CI 產生的 lockfile 中的確切版本後才會打勾。

狀態：**尚未核對**。第一次 CI 成功後，會依 `Cargo.lock`、`package-lock.json` 與 vcpkg 版本逐列確認。

## 隨產品出貨

| 元件 | 用途 | 授權（待核對） | 鎖定版本 | 已核對 |
| --- | --- | --- | --- | --- |
| Tauri 2（tauri、tauri-build、tauri-plugin-dialog） | 桌面外殼、IPC、檔案對話框、打包 | MIT 或 Apache-2.0 | 待 CI | ☐ |
| @tauri-apps/api、@tauri-apps/plugin-dialog | 前端呼叫 Tauri | MIT 或 Apache-2.0 | 待 CI | ☐ |
| React、React DOM | 介面框架 | MIT | 待 CI | ☐ |
| OpenCV 4（core、imgproc、imgcodecs） | 影像解碼與處理 | Apache-2.0 | 待 CI | ☐ |
| libjpeg-turbo、libpng、libtiff、libwebp、zlib（OpenCV 的編解碼相依） | JPEG、PNG、TIFF、WebP 解碼 | 各自的寬鬆授權（BSD 類、libpng、libtiff、BSD-3-Clause、zlib） | 待 CI | ☐ |
| Microsoft WebView2 bootstrapper | 安裝時確保 WebView2 存在 | Microsoft 散布條款 | 待 CI | ☐ |

Rust 與 npm 的間接相依套件數量很多，會在 lockfile 產生後以工具彙整，並附在這份文件之後。

## 僅建置與測試使用（不出貨）

| 元件 | 用途 | 授權（待核對） |
| --- | --- | --- |
| Rust 工具鏈 | 編譯 | MIT 或 Apache-2.0 |
| CMake | C++ 建置 | BSD-3-Clause |
| vcpkg | C++ 相依套件管理 | MIT |
| Node.js、npm | 前端建置 | MIT、Artistic-2.0 |
| TypeScript、Vite、@vitejs/plugin-react、Vitest | 前端建置與測試 | Apache-2.0、MIT、MIT、MIT |
| @tauri-apps/cli | 打包 | MIT 或 Apache-2.0 |

## AI 模型

目前沒有任何模型。每個模型在加入前都必須在 `models/licenses/` 留下紀錄：來源網址、版本、檔案雜湊、授權全文、查核日期。
