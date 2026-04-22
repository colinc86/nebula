function classify(n) {
    switch (true) {
        case n === 0: return "zero";
        case n > 0: return "positive";
        default: return "negative";
    }
}

function weekday(d) {
    switch (d) {
        case 0:
        case 6:
            return "weekend";
        case 1:
        case 2:
        case 3:
        case 4:
        case 5:
            return "weekday";
        default:
            return "unknown";
    }
}

function kindOf(x) {
    if (Array.isArray(x)) return "array";
    if (typeof x === "object" && x !== null) return "object";
    return typeof x;
}
