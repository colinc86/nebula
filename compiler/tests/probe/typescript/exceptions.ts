class BadInput extends Error {
    constructor(msg: string) {
        super(msg);
        this.name = "BadInput";
    }
}

function parseNum(s: string): number {
    const n = Number(s);
    if (isNaN(n)) throw new BadInput(`bad: ${s}`);
    return n;
}

async function fetchJson(url: string): Promise<unknown> {
    try {
        const resp = await fetch(url);
        return await resp.json();
    } catch (e: unknown) {
        if (e instanceof Error) console.error(e.message);
        return null;
    } finally {
        console.log("done");
    }
}
