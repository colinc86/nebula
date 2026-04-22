// array slice
function firstThree(xs) {
    return xs.slice(0, 3);
}

function last(xs) {
    return xs[xs.length - 1];
}

function tail(xs) {
    return xs.slice(1);
}

function dropLast(xs) {
    return xs.slice(0, -1);
}

// string slice
function prefix(s, n) {
    return s.slice(0, n);
}

function suffix(s, n) {
    return s.slice(-n);
}
