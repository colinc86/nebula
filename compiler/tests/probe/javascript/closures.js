function makeAdder(n) {
    return x => x + n;
}

function makeCounter() {
    let count = 0;
    return {
        inc: () => ++count,
        get: () => count,
    };
}

function compose(f, g) {
    return x => f(g(x));
}

function times(n, fn) {
    for (let i = 0; i < n; i++) fn(i);
}

const doubles = [1, 2, 3].map(x => x * 2).filter(x => x > 2);
