function upto(n: number): number {
    let total = 0;
    for (let i = 0; i < n; i++) total += i;
    return total;
}

function indexed<T>(xs: T[]): Array<[number, T]> {
    const out: Array<[number, T]> = [];
    xs.forEach((x, i) => out.push([i, x]));
    return out;
}

function zipped<A, B>(a: A[], b: B[]): Array<[A, B]> {
    const n = Math.min(a.length, b.length);
    const out: Array<[A, B]> = [];
    for (let i = 0; i < n; i++) out.push([a[i], b[i]]);
    return out;
}

function* range(lo: number, hi: number, step: number = 1): Iterable<number> {
    for (let i = lo; i < hi; i += step) yield i;
}
