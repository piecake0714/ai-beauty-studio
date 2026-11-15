# 模型候選清單

查核日期：2026-11-15。本專案不對外販售，授權只需允許非商業使用；但優先選擇也允許商業使用的模型，日後若要販售就不必更換。

這份清單只是**候選**，尚未下載或整合任何模型。下表的授權來自各專案公開頁面的說明；實際採用前，必須對「要下載的那個檔案」逐一確認，並在 `models/licenses/` 留下紀錄（來源網址、版本、檔案雜湊、授權全文、查核日期）。

## 建議採用

| 能力 | 候選 | 公開標示的授權 | 格式與大小 | 來源 |
| --- | --- | --- | --- | --- |
| 臉部偵測 | YuNet（OpenCV Zoo `face_detection_yunet`） | MIT（該目錄 README 載明目錄內所有檔案為 MIT） | ONNX，約 230 KB | https://huggingface.co/opencv/opencv_zoo/blob/d33294bfaec5f2386dab1783e1a4031330645f57/models/face_detection_yunet/README.md |
| 皮膚分割 | MediaPipe Selfie Multiclass Segmentation（輸出含 face-skin、body-skin、hair 等 6 類） | Apache-2.0（模型卡載明） | 原始為 TFLite，256×256 輸入；需轉成 ONNX | https://developers.google.com/edge/mediapipe/solutions/vision/image_segmenter |
| 主體去背 | BiRefNet（ZhengPeng7/BiRefNet 系列） | MIT（專案 LICENSE 與社群 ONNX 轉檔頁面均標示） | 有官方 ONNX；完整版 fp32 約 900 MB，另有 lite 版 | https://github.com/ZhengPeng7/BiRefNet |

## 已排除

| 候選 | 排除原因 |
| --- | --- |
| BRIA RMBG-2.0 | 權重為 CC BY-NC 4.0，僅限非商業使用，且下載需填表同意條款。目前用途雖然允許，但日後若要販售就必須更換，因此不採用。來源：https://huggingface.co/briaai/rmbg-2.0 |

## 尚未解決的問題

1. **BiRefNet 的大小。**（已解決）採用 lite 版：官方 ONNX 為 224,005,088 位元組，安裝程式因此約 174 MB；CI 的 CPU 上 1024×1024 推論約 10 秒。DirectML 上的表現需在使用者電腦確認。
2. **MediaPipe 模型的格式轉換。** 官方只提供 TFLite。需要確認轉成 ONNX 後在 DirectML 與 CPU 上的輸出與原始模型一致；轉換本身在 Apache-2.0 下允許，但要在授權紀錄中註明做過修改。
3. **訓練資料條款。** 上表只確認了模型檔案的授權，尚未查核各模型訓練資料集的使用條款（例如 YuNet 使用的 WIDER FACE）。非商業使用的風險較低，但若日後販售必須補查。
4. **一鍵增強。** 依架構提案先用不需模型的分析演算法實作，因此沒有列候選模型。
