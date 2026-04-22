class BadInput extends Error {
    constructor(msg) {
        super(msg);
        this.name = "BadInput";
    }
}

function parseInt2(s) {
    const n = parseInt(s, 10);
    if (isNaN(n)) throw new BadInput(`bad: ${s}`);
    return n;
}

function safeDivide(a, b) {
    try {
        if (b === 0) throw new Error("div by zero");
        return a / b;
    } catch (e) {
        console.error(e.message);
        return null;
    } finally {
        console.log("done");
    }
}

async function withRetry(fn) {
    try {
        return await fn();
    } catch (e) {
        return await fn();
    }
}
