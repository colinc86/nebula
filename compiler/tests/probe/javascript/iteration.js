function upto(n) {
    let total = 0;
    for (let i = 0; i < n; i++) total += i;
    return total;
}

function indexed(xs) {
    const out = [];
    for (const [i, x] of xs.entries()) out.push([i, x]);
    return out;
}

function zipped(a, b) {
    const n = Math.min(a.length, b.length);
    const out = [];
    for (let i = 0; i < n; i++) out.push([a[i], b[i]]);
    return out;
}

function keysOf(obj) {
    const out = [];
    for (const k in obj) out.push(k);
    return out;
}

function* range(start, stop, step = 1) {
    for (let i = start; i < stop; i += step) yield i;
}
