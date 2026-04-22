trait Shape {
    fn area(&self) -> f64;
    fn describe(&self) -> String {
        format!("shape of area {}", self.area())
    }
}

struct Circle { r: f64 }
struct Square { side: f64 }

impl Shape for Circle {
    fn area(&self) -> f64 { 3.14159 * self.r * self.r }
}

impl Shape for Square {
    fn area(&self) -> f64 { self.side * self.side }
}

// trait object (dyn)
fn total_area(shapes: &[Box<dyn Shape>]) -> f64 {
    shapes.iter().map(|s| s.area()).sum()
}

// supertrait
trait Named { fn name(&self) -> &str; }
trait NamedShape: Shape + Named {}
