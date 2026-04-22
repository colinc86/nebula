// array destructuring
function swap([a, b]) {
    return [b, a];
}

// object destructuring with rename + defaults
function greet({name, age = 18, ...rest}) {
    return `hi ${name}, ${age}, extras=${Object.keys(rest).length}`;
}

// nested destructuring
function firstPair({points: [p, ...rest]}) {
    return [p, rest];
}

// array head/tail
function headTail(xs) {
    const [h, ...t] = xs;
    return {h, t};
}

// multi-return via array
function divmod(a, b) {
    return [Math.floor(a / b), a % b];
}
