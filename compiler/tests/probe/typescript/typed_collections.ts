function sumInts(xs: number[]): number {
    return xs.reduce((a, b) => a + b, 0);
}

function lookup(d: Map<string, number>, k: string): number | undefined {
    return d.get(k);
}

function toSet(xs: string[]): Set<string> {
    return new Set(xs);
}

function makeIndex(words: Array<string>): Record<string, number> {
    const out: Record<string, number> = {};
    words.forEach((w, i) => { out[w] = i; });
    return out;
}

function readonly(xs: ReadonlyArray<number>): number {
    return xs.length;
}
