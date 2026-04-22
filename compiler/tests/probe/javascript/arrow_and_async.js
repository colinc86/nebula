// arrow functions
const double = x => x * 2;
const add = (a, b) => a + b;
const body = (x, y) => {
    const z = x + y;
    return z;
};

// async / await
async function fetchJson(url) {
    const resp = await fetch(url);
    return resp.json();
}

// Promise chain
function chain(x) {
    return Promise.resolve(x)
        .then(v => v + 1)
        .catch(e => console.error(e));
}

// destructuring + spread
function merge({a, b, ...rest}) {
    return {a, b, rest};
}
