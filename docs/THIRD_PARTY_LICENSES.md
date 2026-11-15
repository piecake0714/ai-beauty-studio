# 第三方授權清冊

本專案不對外販售。最後核對：2026-11-15，依 CI run 43674211393（commit `797df80`）產生的報告。

**結論：** 隨產品出貨的套件中，沒有 GPL、AGPL 等強著作權保留（strong copyleft）授權，也沒有僅限非商業使用的授權。需要注意的只有 4 個 MPL-2.0 套件（見下方）。

## C++ 套件（vcpkg，靜態連結進 `aibs_engine.dll`）

vcpkg 的 SPDX 檔對這 8 個套件的授權欄位都是 `NOASSERTION`，因此改以 CI 輸出的各套件 copyright 檔（CI run 43668844544）逐一核對。

| 套件 | 鎖定版本 | 授權 | 已以 copyright 檔核對 |
| --- | --- | --- | --- |
| opencv4（core、imgproc、imgcodecs） | 4.14.0 | Apache-2.0 | ☑ |
| libjpeg-turbo | 3.2.0#1 | IJG、BSD-3-Clause、Zlib | ☑ |
| libpng | 1.6.59 | libpng-2.0 | ☑ |
| libspng | 0.7.4#1 | BSD-2-Clause | ☑ |
| tiff（libtiff） | 4.7.2 | libtiff | ☑ |
| libwebp | 1.6.0#3 | BSD-3-Clause | ☑ |
| liblzma（xz） | 5.8.4 | 0BSD（xz 套件中的 GPL／LGPL 部分只適用於命令列工具與腳本，未出貨） | ☑ |
| zlib | 1.3.2#2 | Zlib | ☑ |

## AI 執行環境（隨安裝程式出貨的 DLL）

| 元件 | 鎖定版本 | 授權 | 來源與核對 |
| --- | --- | --- | --- |
| ONNX Runtime（Microsoft.ML.OnnxRuntime.DirectML） | 1.24.4，NuGet 套件 SHA-256 `57e9f11b…d788681` | MIT | 套件內 `LICENSE`；其第三方聲明 `ThirdPartyNotices.txt` 一併收入聲明檔 |
| DirectML（Microsoft.AI.DirectML） | 1.15.4，NuGet 套件 SHA-256 `4e7cb7dd…3615b9` | Microsoft Software License Terms for DirectML | 條款允許將其複製並散布於自行開發的應用程式中，不得單獨散布、不得移除 Microsoft 聲明（[授權頁面](https://www.nuget.org/packages/Microsoft.AI.DirectML/1.15.4/License)）；`LICENSE.txt` 與 `ThirdPartyNotices.txt` 一併收入聲明檔 |
| YuNet 臉部偵測模型（OpenCV Zoo，face_detection_yunet_2023mar.onnx） | 2023mar，SHA-256 `8f2383e4…2552fa4` | MIT | 不存入 Git，由 `scripts/fetch_models.py` 從 Hugging Face 的 OpenCV Zoo 鏡像下載並驗證雜湊；授權紀錄 `models/licenses/face-yunet.json`，上游 LICENSE 全文收入聲明檔 |
| BiRefNet lite 主體分離模型（BiRefNet-general-bb_swin_v1_tiny-epoch_232.onnx） | 官方 GitHub release v1，SHA-256 `56000243…d0f03333` | MIT | 不存入 Git，由 `scripts/fetch_models.py` 從 https://github.com/ZhengPeng7/BiRefNet/releases/tag/v1 下載並驗證雜湊；授權紀錄 `models/licenses/background-birefnet-lite.json`，上游 LICENSE 全文收入聲明檔。訓練資料集條款尚未查核 |
| Depth Anything V2 Small 深度模型（onnx-community 轉檔，onnx/model.onnx） | SHA-256 `afb6a5c2…1df10c`，99,060,839 位元組 | Apache-2.0（只有 Small 版；Base／Large／Giant 為 CC-BY-NC-4.0，未使用） | 不存入 Git，由 `scripts/fetch_models.py` 下載並驗證雜湊；上游 LICENSE（Apache License 2.0 全文）收入聲明檔；授權紀錄 `models/licenses/depth-anything-v2-small.json`。訓練資料集條款尚未查核 |

## Rust 套件（連結進主程式）

共 207 個外部套件（另有本專案自己的套件，不對外發布）。清單以 cargo-about 依實際啟用的 features 計算為準，與 CI run 43674211393 實際編譯的套件一致；完整授權全文收錄在隨產品出貨的 `THIRD_PARTY_NOTICES.txt`。

| 授權類別 | 套件數 | 說明 |
| --- | --- | --- |
| MIT 及／或 Apache-2.0（含寫法不同的變體） | 178 | 寬鬆授權，保留授權聲明即可 |
| Unicode-3.0 | 19 | 寬鬆授權（ICU4X 系列，Tauri 經 URL 處理間接使用） |
| Zlib | 1 | 寬鬆授權 |
| BSD-3-Clause | 4 | 寬鬆授權 |
| **MPL-2.0** | **5** | **檔案層級的弱著作權保留，見下方** |

**需要注意：MPL-2.0。** `cssparser` 0.37.0、`cssparser-macros` 0.7.1、`dtoa-short` 0.3.5、`option-ext` 0.2.0、`selectors` 0.38.0。這些是 Tauri 經由 HTML/CSS 解析間接引入的。MPL-2.0 只要求「修改過的 MPL 檔案」須公開原始碼；我們沒有修改它們，所以只需附上授權聲明（已收錄於 `THIRD_PARTY_NOTICES.txt`），並告知可取得原始碼的位置。對不販售的使用沒有實際限制。

Unicode-3.0 套件：`icu_collections` 2.4.0、`icu_locale_core` 2.4.0、`icu_normalizer` 2.4.0、`icu_normalizer_data` 2.4.0、`icu_properties` 2.4.0、`icu_properties_data` 2.4.0、`icu_provider` 2.4.0、`litemap` 0.8.3、`potential_utf` 0.1.6、`tinystr` 0.8.5、`unicode-ident` 1.0.27、`writeable` 0.6.4、`yoke` 0.8.4、`yoke-derive` 0.8.4、`zerofrom` 0.1.8、`zerofrom-derive` 0.1.8、`zerotrie` 0.2.5、`zerovec` 0.11.9、`zerovec-derive` 0.11.7。

## 前端（打包進安裝程式）

| 套件 | 鎖定版本 | 授權（package-lock.json） |
| --- | --- | --- |
| react | 18.3.1 | MIT |
| react-dom | 18.3.1 | MIT |
| scheduler | 0.23.2 | MIT |
| loose-envify | 1.4.0 | MIT |
| js-tokens | 4.0.0 | MIT |
| @tauri-apps/api | 2.12.4 | Apache-2.0 OR MIT |
| @tauri-apps/plugin-dialog | 2.8.4 | MIT OR Apache-2.0 |

## 其他隨安裝程式出貨的元件

| 元件 | 授權 | 備註 |
| --- | --- | --- |
| Microsoft WebView2 bootstrapper | Microsoft 散布條款 | 安裝時下載 WebView2 runtime；條款尚未逐條核對 |

## 僅建置與測試使用（不出貨）

Rust 工具鏈、CMake、vcpkg、Node.js、npm、TypeScript、Vite、Vitest、@tauri-apps/cli。這些不隨產品散布，不需附授權聲明。

## AI 模型

目前沒有任何模型。候選見 `models/CANDIDATES.md`；每個模型實際加入前，都必須在 `models/licenses/` 留下紀錄。

## 授權聲明檔

CI 以 `scripts/build_notices.py` 產生 `THIRD_PARTY_NOTICES.txt`，內含所有出貨元件的授權全文，隨安裝程式出貨，並可在「設定 → 關於 → 第三方授權聲明」檢視。任何出貨元件缺少授權文字時，建置會失敗。

尚未完成：Microsoft WebView2 bootstrapper 的散布條款尚未逐條核對。
