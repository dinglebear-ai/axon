export function splitShellWords(input: string): string[] {
  const words: string[] = [];
  let current = "";
  let quote: "'" | '"' | null = null;

  for (let i = 0; i < input.length; i += 1) {
    const ch = input[i];
    if ((ch === "'" || ch === '"') && quote === null) {
      quote = ch;
    } else if (ch === quote) {
      quote = null;
    } else if (ch === "\\" && i + 1 < input.length) {
      i += 1;
      current += input[i];
    } else if (/\s/.test(ch) && quote === null) {
      if (current) {
        words.push(current);
        current = "";
      }
    } else {
      current += ch;
    }
  }

  if (quote) {
    throw new Error(`unterminated ${quote} quote`);
  }
  if (current) words.push(current);
  return words;
}
