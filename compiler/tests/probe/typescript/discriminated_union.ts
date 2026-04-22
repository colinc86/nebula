type Shape =
    | { kind: "circle"; r: number }
    | { kind: "square"; side: number }
    | { kind: "rect"; w: number; h: number };

function area(s: Shape): number {
    switch (s.kind) {
        case "circle": return Math.PI * s.r * s.r;
        case "square": return s.side * s.side;
        case "rect":   return s.w * s.h;
    }
}

type Result<T, E> =
    | { ok: true; value: T }
    | { ok: false; error: E };

function parseNum(s: string): Result<number, string> {
    const n = Number(s);
    return isNaN(n) ? {ok: false, error: `bad: ${s}`} : {ok: true, value: n};
}
