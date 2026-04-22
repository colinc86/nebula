async function fetchUser(id: number): Promise<string> {
    return `user_${id}`;
}

async function fetchAll(ids: number[]): Promise<string[]> {
    return await Promise.all(ids.map(id => fetchUser(id)));
}

async function withRetry<T>(fn: () => Promise<T>, n: number): Promise<T> {
    let lastErr: unknown;
    for (let i = 0; i < n; i++) {
        try {
            return await fn();
        } catch (e) {
            lastErr = e;
        }
    }
    throw lastErr;
}

async function* stream(n: number): AsyncIterable<number> {
    for (let i = 0; i < n; i++) yield i;
}
