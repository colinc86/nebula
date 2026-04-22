const makeAdder = (n: number) => (x: number): number => x + n;

function compose<A, B, C>(f: (b: B) => C, g: (a: A) => B): (a: A) => C {
    return a => f(g(a));
}

function makeCounter(start: number): { inc: () => number; get: () => number } {
    let n = start;
    return {
        inc: () => ++n,
        get: () => n,
    };
}

const doubled = [1, 2, 3]
    .map((x: number) => x * 2)
    .filter((x: number) => x > 2)
    .reduce((a, b) => a + b, 0);
