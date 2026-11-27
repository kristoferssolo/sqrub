#[cfg_attr(
    not(test),
    expect(dead_code, reason = "wired into the CLI in a later step")
)]
mod extract;
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "wired into the CLI in a later step")
)]
mod literal;

fn main() {
    println!("Hello, world!");
}
