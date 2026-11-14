/**
 * UI strings in Traditional Chinese and English. Every key must exist in
 * both tables; `i18n.test.ts` enforces that.
 */
export type Language = "zh-Hant" | "en";

const zhHant = {
  openImage: "開啟影像",
  opening: "開啟中…",
  emptyHint: "尚未開啟影像。請按「開啟影像」選擇 JPEG、PNG、TIFF 或 WebP 檔案。",
  imageFiles: "影像檔",
  sizeLabel: "原始尺寸",
  "error.not_found": "找不到這個檔案，它可能已被移動或刪除。",
  "error.read_failed": "無法讀取這個檔案，請確認您有權限開啟它。",
  "error.not_an_image": "這個檔案已損毀，或不是支援的影像格式。",
  "error.out_of_memory": "記憶體不足，無法開啟這張影像。",
  "error.internal": "開啟影像時發生內部錯誤。",
};

const en: Record<keyof typeof zhHant, string> = {
  openImage: "Open image",
  opening: "Opening…",
  emptyHint: "No image open. Choose Open image to select a JPEG, PNG, TIFF or WebP file.",
  imageFiles: "Image files",
  sizeLabel: "Original size",
  "error.not_found": "The file could not be found. It may have been moved or deleted.",
  "error.read_failed": "The file could not be read. Check that you have permission to open it.",
  "error.not_an_image": "The file is damaged or is not a supported image format.",
  "error.out_of_memory": "There is not enough memory to open this image.",
  "error.internal": "An internal error occurred while opening the image.",
};

export type MessageKey = keyof typeof zhHant;

export const tables: Record<Language, Record<MessageKey, string>> = {
  "zh-Hant": zhHant,
  en,
};

export function detectLanguage(locale: string): Language {
  return locale.toLowerCase().startsWith("zh") ? "zh-Hant" : "en";
}

export function translate(language: Language, key: MessageKey): string {
  return tables[language][key];
}

/** Message for an error code returned by the core; unknown codes map to the internal error. */
export function errorMessage(language: Language, code: string): string {
  const key = `error.${code}`;
  return key in tables[language]
    ? tables[language][key as MessageKey]
    : tables[language]["error.internal"];
}
