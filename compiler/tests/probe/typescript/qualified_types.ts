import * as fs from "node:fs";
import * as path from "node:path";

function readFile(p: string): Buffer {
    return fs.readFileSync(p);
}

function joinPath(a: string, b: string): string {
    return path.join(a, b);
}

// namespaced type
namespace Geom {
    export interface Point { x: number; y: number; }
    export function dist(a: Point, b: Point): number {
        return Math.hypot(a.x - b.x, a.y - b.y);
    }
}

function origin(): Geom.Point {
    return {x: 0, y: 0};
}
