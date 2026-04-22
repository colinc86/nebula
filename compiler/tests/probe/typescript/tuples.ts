// fixed-shape tuple
function makePair(a: number, b: string): [number, string] {
    return [a, b];
}

// destructure tuple return
function divmod(a: number, b: number): [number, number] {
    return [Math.floor(a / b), a % b];
}

const [q, r] = divmod(10, 3);

// labeled tuples
function rangeBounds(): [lo: number, hi: number] {
    return [0, 10];
}

// variadic tuple
function head<T, R extends unknown[]>(xs: [T, ...R]): T {
    return xs[0];
}
