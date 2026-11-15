import { describe, expect, it } from "vitest";
import errorCodes from "./error-codes.json";
import { detectLanguage, errorMessage, tables } from "./i18n";

describe("i18n", () => {
  it("has the same keys, all non-empty, in both languages", () => {
    const zhKeys = Object.keys(tables["zh-Hant"]).sort();
    const enKeys = Object.keys(tables.en).sort();
    expect(enKeys).toEqual(zhKeys);
    for (const table of Object.values(tables)) {
      for (const text of Object.values(table)) {
        expect(text.trim().length).toBeGreaterThan(0);
      }
    }
  });

  it("picks Traditional Chinese for Chinese locales and English otherwise", () => {
    expect(detectLanguage("zh-TW")).toBe("zh-Hant");
    expect(detectLanguage("zh-Hant-HK")).toBe("zh-Hant");
    expect(detectLanguage("en-US")).toBe("en");
    expect(detectLanguage("ja-JP")).toBe("en");
  });

  it("maps every core error code to a message and falls back for unknown codes", () => {
    for (const code of ["not_found", "read_failed", "not_an_image", "out_of_memory", "internal"]) {
      expect(errorMessage("en", code)).toBe(tables.en[`error.${code}` as keyof typeof tables.en]);
    }
    expect(errorMessage("zh-Hant", "something_new")).toBe(tables["zh-Hant"]["error.internal"]);
  });

  it("has a message in every language for every error code the core can send", () => {
    expect(errorCodes.codes.length).toBeGreaterThan(20);
    for (const table of Object.values(tables)) {
      for (const code of errorCodes.codes) {
        expect(Object.keys(table), `error.${code}`).toContain(`error.${code}`);
      }
    }
  });
});
