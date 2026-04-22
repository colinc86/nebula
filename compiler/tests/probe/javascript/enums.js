// frozen object as enum
const Color = Object.freeze({
    RED: "red",
    GREEN: "green",
    BLUE: "blue",
});

const Level = Object.freeze({
    DEBUG: 0,
    INFO: 1,
    WARN: 2,
    ERROR: 3,
});

function label(c) {
    if (c === Color.RED) return "r";
    return "x";
}
