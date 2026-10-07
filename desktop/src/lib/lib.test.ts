import { describe, expect, it } from "vitest";
import {
  baseName,
  formatAgo,
  formatBytes,
  formatCount,
  formatDuration,
  pageRange,
  plural,
} from "./format";
import { tokenize, type Language } from "./highlight";
import { MASK, maskUrlPassword, urlHasPassword } from "./mask";

describe("maskUrlPassword", () => {
  it("hides the password and nothing else", () => {
    expect(maskUrlPassword("postgres://alice:s3cret@db.internal:5432/app")).toBe(
      `postgres://alice:${MASK}@db.internal:5432/app`,
    );
  });

  it("leaves URLs without a password alone", () => {
    for (const url of [
      "postgres://alice@db.internal/app",
      "postgres://db.internal/app",
      "postgres://alice:@db.internal/app",
      "not a url",
      "",
    ]) {
      expect(maskUrlPassword(url)).toBe(url);
      expect(urlHasPassword(url)).toBe(false);
    }
  });

  it("hides passwords that contain @, / or :", () => {
    for (const secret of ["p@ss", "pa/ss", "a:b:c", "p%40ss"]) {
      const masked = maskUrlPassword(`postgres://bob:${secret}@host/db?sslmode=require`);
      expect(masked).not.toContain(secret);
      expect(masked).toBe(`postgres://bob:${MASK}@host/db?sslmode=require`);
    }
  });

  it("still hides a password while the rest of the URL is being typed", () => {
    expect(maskUrlPassword("postgres://bob:hunter2@")).toBe(`postgres://bob:${MASK}@`);
  });
});

describe("format", () => {
  it("counts and pluralises", () => {
    expect(formatCount(1234567)).toBe("1,234,567");
    expect(plural(1, "row")).toBe("1 row");
    expect(plural(0, "row")).toBe("0 rows");
    expect(plural(2500, "table")).toBe("2,500 tables");
  });

  it("formats sizes and durations", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(8192)).toBe("8.0 KB");
    expect(formatBytes(5 * 1024 * 1024 * 1024)).toBe("5.0 GB");
    expect(formatDuration(0)).toBe("<1 ms");
    expect(formatDuration(42)).toBe("42 ms");
    expect(formatDuration(1530)).toBe("1.53 s");
    expect(formatDuration(125_000)).toBe("2 min 5 s");
  });

  it("describes page ranges from the row offset", () => {
    expect(pageRange(0, 50)).toBe("1–50");
    expect(pageRange(100, 20)).toBe("101–120");
    expect(pageRange(0, 0)).toBe("0");
  });

  it("describes how long ago something happened", () => {
    const now = Date.UTC(2026, 9, 7, 12, 0, 0);
    const seconds = Math.floor(now / 1000);
    expect(formatAgo(seconds - 5, now)).toBe("just now");
    expect(formatAgo(seconds - 600, now)).toBe("10 min ago");
    expect(formatAgo(seconds - 7200, now)).toBe("2 h ago");
    expect(formatAgo(seconds - 3 * 86_400, now)).toBe("3 d ago");
  });

  it("takes the last segment of a path with either separator", () => {
    expect(baseName("C:\\data\\shop.db")).toBe("shop.db");
    expect(baseName("/home/me/project/")).toBe("project");
    expect(baseName("shop.db")).toBe("shop.db");
  });
});

describe("tokenize", () => {
  const kinds = (source: string, language: Language) =>
    tokenize(source, language)
      .filter((token) => token.kind !== "plain")
      .map((token) => `${token.kind}:${token.text}`);

  it("never changes the text it colours", () => {
    const samples: [string, Language][] = [
      ['table users {\n  id: int [primary]\n  name: string = "x" // note\n}\n', "kairo"],
      ["table \"Order Items\" { \"unit price\": float = -9.5 }", "kairo"],
      ["SELECT a, 'it''s' FROM t -- c\n/* b */ WHERE x = 1.5;", "sql"],
      ["", "sql"],
      ["unterminated 'string", "sql"],
      ["日本語 ñ 'é'", "sql"],
    ];
    for (const [source, language] of samples) {
      expect(tokenize(source, language).map((token) => token.text).join("")).toBe(source);
    }
  });

  it("colours a schema like the website does", () => {
    expect(
      kinds('table users {\n  id: int [primary]\n  active: bool = true\n  note: money // odd\n}', "kairo"),
    ).toEqual([
      "kw:table",
      "ent:users",
      "tp:int",
      "kw:primary",
      "tp:bool",
      "vl:true",
      "cm:// odd",
    ]);
  });

  it("does not treat a field called table or primary as a keyword", () => {
    expect(kinds("table t { primary: string, required: int }", "kairo")).toEqual([
      "kw:table",
      "ent:t",
      "tp:string",
      "tp:int",
    ]);
  });

  it("colours SQL keywords, strings, numbers and comments", () => {
    expect(kinds("select name from users where id = 42 and note = 'drop' -- why", "sql")).toEqual([
      "kw:select",
      "kw:from",
      "kw:where",
      "vl:42",
      "kw:and",
      "vl:'drop'",
      "cm:-- why",
    ]);
  });

  it("leaves keywords inside quoted names and identifiers alone", () => {
    expect(kinds('SELECT "select", selection, t1 FROM t', "sql")).toEqual(["kw:SELECT", "kw:FROM"]);
  });
});
