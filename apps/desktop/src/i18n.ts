/**
 * UI strings in Traditional Chinese and English. Every key must exist in
 * both tables; `i18n.test.ts` enforces that. `{name}` marks a value filled
 * in by `translate`.
 */
import type { LanguagePreference } from "./types";

export type Language = "zh-Hant" | "en";

const zhHant = {
  appName: "AI Beauty Studio",
  "nav.home": "首頁",
  "nav.editor": "編輯器",
  openImage: "開啟影像",
  opening: "開啟中…",
  settings: "設定",
  close: "關閉",
  dismiss: "關閉訊息",
  imageFiles: "影像檔",
  "home.title": "歡迎使用 AI Beauty Studio",
  "home.subtitle": "開啟一張 JPEG、PNG、TIFF 或 WebP 照片開始。快速鍵：Ctrl+O。",
  "home.recent": "最近開啟",
  "home.clearRecent": "清除清單",
  "home.noRecent": "尚無最近開啟的檔案。",
  "home.remove": "從清單移除",
  "home.removeNamed": "從清單移除 {name}",
  "home.missing": "找不到檔案",
  "home.noPreview": "無法預覽",
  "editor.empty": "尚未開啟影像。",
  "info.title": "影像資訊",
  "info.name": "檔名",
  "info.format": "格式",
  "info.dimensions": "原始尺寸",
  "info.size": "檔案大小",
  "info.folder": "位置",
  "info.unknown": "未知",
  "status.ready": "就緒",
  "status.opening": "開啟中…",
  "status.rendering": "更新畫面中…",
  "zoom.percent": "{percent}%",
  "zoom.fit": "符合視窗",
  "zoom.actual": "100%",
  "zoom.in": "放大",
  "zoom.out": "縮小",
  "zoom.label": "縮放比例",
  "zoom.hint": "滾輪縮放，拖曳平移。Ctrl+0 符合視窗，Ctrl+1 為 100%。",
  "settings.theme": "主題",
  "settings.language": "語言",
  "settings.system": "跟隨系統",
  "settings.light": "淺色",
  "settings.dark": "深色",
  "settings.about": "關於",
  "settings.version": "版本 {version}",
  "settings.notices": "第三方授權聲明",
  "settings.back": "返回設定",
  "settings.recovered": "設定檔已損毀，已恢復為預設值。原檔案已保留為 settings.json.damaged。",
  "error.not_found": "找不到這個檔案，它可能已被移動或刪除。",
  "error.read_failed": "無法讀取這個檔案，請確認您有權限開啟它。",
  "error.not_an_image": "這個檔案已損毀，或不是支援的影像格式。",
  "error.out_of_memory": "記憶體不足，無法開啟這張影像。",
  "error.settings_save_failed": "無法儲存設定。本次變更在關閉程式後不會保留。",
  "error.notices_missing": "找不到授權聲明檔案，安裝可能不完整。",
  "error.internal": "發生內部錯誤。",
};

const en: Record<keyof typeof zhHant, string> = {
  appName: "AI Beauty Studio",
  "nav.home": "Home",
  "nav.editor": "Editor",
  openImage: "Open image",
  opening: "Opening…",
  settings: "Settings",
  close: "Close",
  dismiss: "Dismiss message",
  imageFiles: "Image files",
  "home.title": "Welcome to AI Beauty Studio",
  "home.subtitle": "Open a JPEG, PNG, TIFF or WebP photo to begin. Shortcut: Ctrl+O.",
  "home.recent": "Recent",
  "home.clearRecent": "Clear list",
  "home.noRecent": "No recently opened files.",
  "home.remove": "Remove from list",
  "home.removeNamed": "Remove {name} from list",
  "home.missing": "File not found",
  "home.noPreview": "No preview",
  "editor.empty": "No image open.",
  "info.title": "Image information",
  "info.name": "Name",
  "info.format": "Format",
  "info.dimensions": "Original size",
  "info.size": "File size",
  "info.folder": "Location",
  "info.unknown": "Unknown",
  "status.ready": "Ready",
  "status.opening": "Opening…",
  "status.rendering": "Updating view…",
  "zoom.percent": "{percent}%",
  "zoom.fit": "Fit",
  "zoom.actual": "100%",
  "zoom.in": "Zoom in",
  "zoom.out": "Zoom out",
  "zoom.label": "Zoom",
  "zoom.hint": "Scroll to zoom, drag to pan. Ctrl+0 fits the window, Ctrl+1 shows 100%.",
  "settings.theme": "Theme",
  "settings.language": "Language",
  "settings.system": "Use system setting",
  "settings.light": "Light",
  "settings.dark": "Dark",
  "settings.about": "About",
  "settings.version": "Version {version}",
  "settings.notices": "Third-party notices",
  "settings.back": "Back to settings",
  "settings.recovered": "The settings file was damaged and has been reset. The old file was kept as settings.json.damaged.",
  "error.not_found": "The file could not be found. It may have been moved or deleted.",
  "error.read_failed": "The file could not be read. Check that you have permission to open it.",
  "error.not_an_image": "The file is damaged or is not a supported image format.",
  "error.out_of_memory": "There is not enough memory to open this image.",
  "error.settings_save_failed": "Settings could not be saved. This change will be lost when the app closes.",
  "error.notices_missing": "The license notices file is missing. The installation may be incomplete.",
  "error.internal": "An internal error occurred.",
};

export type MessageKey = keyof typeof zhHant;

export const tables: Record<Language, Record<MessageKey, string>> = {
  "zh-Hant": zhHant,
  en,
};

export function detectLanguage(locale: string): Language {
  return locale.toLowerCase().startsWith("zh") ? "zh-Hant" : "en";
}

export function resolveLanguage(preference: LanguagePreference, systemLocale: string): Language {
  return preference === "system" ? detectLanguage(systemLocale) : preference;
}

export function translate(
  language: Language,
  key: MessageKey,
  values: Record<string, string | number> = {},
): string {
  return tables[language][key].replace(/\{(\w+)\}/g, (match, name: string) =>
    name in values ? String(values[name]) : match,
  );
}

/** Message for an error code from the core; unknown codes map to the internal error. */
export function errorMessage(language: Language, code: string): string {
  const key = `error.${code}`;
  return key in tables[language]
    ? tables[language][key as MessageKey]
    : tables[language]["error.internal"];
}
