function firstN<T>(xs: T[], n: number): T[] {
    return xs.slice(0, n);
}

function last<T>(xs: T[]): T | undefined {
    return xs[xs.length - 1];
}

function tail<T>(xs: T[]): T[] {
    return xs.slice(1);
}

function between<T>(xs: T[], lo: number, hi: number): T[] {
    return xs.slice(lo, hi);
}

function substring(s: string, n: number): string {
    return s.slice(0, n);
}
