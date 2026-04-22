function headOpt<T>(xs: T[]): T | undefined {
    return xs.length > 0 ? xs[0] : undefined;
}

function getOr<T>(x: T | null | undefined, fallback: T): T {
    return x ?? fallback;
}

interface User { name: string; email?: string; }

function displayEmail(u: User): string {
    return u.email ?? "no email";
}

function len(s?: string): number {
    return s?.length ?? 0;
}
