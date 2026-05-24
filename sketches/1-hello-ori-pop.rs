use oripop_canvas::prelude::*;

fn main() {
    size(900, 700);
    title("hello-ori-pop");
    run(draw);
}

fn draw() {
    background(15, 15, 20);
    stroke(255, 100, 50);
    stroke_weight(3.0);
    line(100.0, 100.0, 800.0, 600.0);
}
