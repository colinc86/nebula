class Animal {
    constructor(name) {
        this.name = name;
    }
    speak() {
        return `${this.name} makes a sound`;
    }
}

class Dog extends Animal {
    constructor(name) {
        super(name);
    }
    speak() {
        return `${this.name} barks`;
    }
}

// getters / setters
class Temperature {
    constructor(c) { this._c = c; }
    get fahrenheit() { return this._c * 9 / 5 + 32; }
    set fahrenheit(f) { this._c = (f - 32) * 5 / 9; }
}
