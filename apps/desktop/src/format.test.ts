import { describe, expect, it } from "vitest";
import { fileName, folderOf, formatBytes, resolveTheme } from "./format";
import { resolveLanguage, translate } from "./i18n";

describe("paths", () => {
  it("splits Windows and POSIX paths", () => {
    expect(fileName("C:\\照片\\旅行\\一.jpg")).toBe("一.jpg");
    expect(folderOf("C:\\照片\\旅行\\一.jpg")).toBe("C:\\照片\\旅行");
    expect(fileName("/home/a/b.png")).toBe("b.png");
    expect(folderOf("b.png")).toBe("");
  });
});

describe("formatBytes", () => {
  it("picks a readable unit", () => {
    expect(formatBytes(512, "en")).toBe("512 B");
    expect(formatBytes(1536, "en")).toBe("1.5 KB");
    expect(formatBytes(5 * 1024 * 1024, "en")).toBe("5.0 MB");
    expect(formatBytes(250 * 1024 * 1024, "en")).toBe("250 MB");
  });
});

describe("preferences", () => {
  it("resolves the theme, following the system when asked", () => {
    expect(resolveTheme("system", true)).toBe("dark");
    expect(resolveTheme("system", false)).toBe("light");
    expect(resolveTheme("light", true)).toBe("light");
    expect(resolveTheme("dark", false)).toBe("dark");
  });

  it("resolves the language, following the system when asked", () => {
    expect(resolveLanguage("system", "zh-TW")).toBe("zh-Hant");
    expect(resolveLanguage("system", "fr-FR")).toBe("en");
    expect(resolveLanguage("en", "zh-TW")).toBe("en");
    expect(resolveLanguage("zh-Hant", "en-US")).toBe("zh-Hant");
  });

  it("fills in placeholders", () => {
    expect(translate("en", "zoom.percent", { percent: 25 })).toBe("25%");
    expect(translate("zh-Hant", "settings.version", { version: "0.1.0" })).toBe("版本 0.1.0");
  });
});
