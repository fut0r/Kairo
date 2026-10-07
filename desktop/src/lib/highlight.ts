// Syntax colouring for the editors. This is cosmetic: whether a schema is
// valid is always decided by the Rust parser, never by these tokenizers.

export type TokenKind = "kw" | "ent" | "tp" | "vl" | "cm" | "plain";

export interface Token {
  kind: TokenKind;
  text: string;
}

export type Language = "sql" | "kairo";

const KAIRO_TYPES = new Set(["string", "int", "float", "bool", "blob", "timestamp"]);
const KAIRO_MODIFIERS = new Set(["required", "primary", "unique"]);

const SQL_KEYWORDS = new Set(
  (
    "select from where and or not null is in like ilike between exists as on join left right " +
    "inner outer full cross natural using group by order having limit offset union all " +
    "intersect except distinct insert into values update set delete create table view index " +
    "unique primary key foreign references default check constraint drop alter add column " +
    "rename to if begin commit rollback transaction savepoint release with recursive case " +
    "when then else end asc desc returning truncate replace conflict do nothing explain " +
    "analyze pragma vacuum grant revoke cascade restrict temporary temp trigger"
  ).split(" "),
);

function push(tokens: Token[], kind: TokenKind, text: string): void {
  if (text === "") return;
  const last = tokens[tokens.length - 1];
  if (last && last.kind === kind) {
    last.text += text;
  } else {
    tokens.push({ kind, text });
  }
}

const WORD = /[A-Za-z_][A-Za-z0-9_]*/y;
const NUMBER = /-?\d+(\.\d+)?/y;

function matchAt(pattern: RegExp, source: string, index: number): string | null {
  pattern.lastIndex = index;
  const found = pattern.exec(source);
  return found ? found[0] : null;
}

/** Reads a quoted run starting at `index`, where `quote` doubled is an escape. */
function quoted(source: string, index: number, quote: string, doubling: boolean): string {
  let end = index + 1;
  while (end < source.length) {
    if (source[end] === quote) {
      if (doubling && source[end + 1] === quote) {
        end += 2;
        continue;
      }
      end += 1;
      break;
    }
    end += 1;
  }
  return source.slice(index, end);
}

function tokenizeKairo(source: string): Token[] {
  const tokens: Token[] = [];
  let i = 0;
  // What the next name means: a table's name, or a field's type.
  let expect: "table-name" | "type" | null = null;
  let inModifiers = false;

  while (i < source.length) {
    const ch = source[i]!;

    if (ch === "/" && source[i + 1] === "/") {
      const end = source.indexOf("\n", i);
      const stop = end < 0 ? source.length : end;
      push(tokens, "cm", source.slice(i, stop));
      i = stop;
      continue;
    }

    if (ch === '"') {
      const text = quoted(source, i, '"', false);
      // A quoted run is a name where a name is expected, otherwise a value.
      push(tokens, expect === "table-name" ? "ent" : "vl", text);
      if (expect === "table-name") expect = null;
      i += text.length;
      continue;
    }

    const word = matchAt(WORD, source, i);
    if (word) {
      if (expect === "table-name") {
        push(tokens, "ent", word);
        expect = null;
      } else if (expect === "type") {
        push(tokens, KAIRO_TYPES.has(word) ? "tp" : "plain", word);
        expect = null;
      } else if (inModifiers && KAIRO_MODIFIERS.has(word)) {
        push(tokens, "kw", word);
      } else if (word === "table") {
        push(tokens, "kw", word);
        expect = "table-name";
      } else if (word === "true" || word === "false") {
        push(tokens, "vl", word);
      } else {
        push(tokens, "plain", word);
      }
      i += word.length;
      continue;
    }

    const number = matchAt(NUMBER, source, i);
    if (number) {
      push(tokens, "vl", number);
      i += number.length;
      continue;
    }

    if (ch === ":") expect = "type";
    if (ch === "[") inModifiers = true;
    if (ch === "]") inModifiers = false;
    push(tokens, "plain", ch);
    i += 1;
  }

  return tokens;
}

function tokenizeSql(source: string): Token[] {
  const tokens: Token[] = [];
  let i = 0;

  while (i < source.length) {
    const ch = source[i]!;

    if (ch === "-" && source[i + 1] === "-") {
      const end = source.indexOf("\n", i);
      const stop = end < 0 ? source.length : end;
      push(tokens, "cm", source.slice(i, stop));
      i = stop;
      continue;
    }

    if (ch === "/" && source[i + 1] === "*") {
      const end = source.indexOf("*/", i + 2);
      const stop = end < 0 ? source.length : end + 2;
      push(tokens, "cm", source.slice(i, stop));
      i = stop;
      continue;
    }

    if (ch === "'") {
      const text = quoted(source, i, "'", true);
      push(tokens, "vl", text);
      i += text.length;
      continue;
    }

    if (ch === '"' || ch === "`") {
      const text = quoted(source, i, ch, true);
      push(tokens, "plain", text);
      i += text.length;
      continue;
    }

    const word = matchAt(WORD, source, i);
    if (word) {
      push(tokens, SQL_KEYWORDS.has(word.toLowerCase()) ? "kw" : "plain", word);
      i += word.length;
      continue;
    }

    const number = matchAt(NUMBER, source, i);
    if (number && !/[A-Za-z0-9_]/.test(source[i - 1] ?? "")) {
      push(tokens, "vl", number);
      i += number.length;
      continue;
    }

    push(tokens, "plain", ch);
    i += 1;
  }

  return tokens;
}

/** Splits source into coloured runs. Joining the token text gives the source back. */
export function tokenize(source: string, language: Language): Token[] {
  return language === "kairo" ? tokenizeKairo(source) : tokenizeSql(source);
}
