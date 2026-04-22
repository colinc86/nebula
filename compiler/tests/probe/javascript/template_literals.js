function greet(name, age) {
    return `hi ${name}, you are ${age}`;
}

function multi(a, b) {
    return `sum=${a + b}
prod=${a * b}`;
}

function tag(strings, ...values) {
    let out = "";
    strings.forEach((s, i) => {
        out += s;
        if (i < values.length) out += `[${values[i]}]`;
    });
    return out;
}

const greeting = tag`hello ${"world"}, today is ${new Date()}`;
