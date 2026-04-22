// generic function
function map<T, U>(xs: T[], f: (x: T) => U): U[] {
    return xs.map(f);
}

// interface with optional fields
interface User {
    id: number;
    name: string;
    email?: string;
}

// union / intersection
type Id = string | number;
type Named = { name: string };
type Stamped = { ts: number };
type Event = Named & Stamped;

// class with generic + arrow property
class Cache<K, V> {
    private store = new Map<K, V>();
    get = (k: K): V | undefined => this.store.get(k);
    set(k: K, v: V): void {
        this.store.set(k, v);
    }
}

// enum
enum Color { Red, Green, Blue }
