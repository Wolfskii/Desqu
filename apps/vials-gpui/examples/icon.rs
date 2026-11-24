fn main() {
    let id = std::env::args().nth(1).unwrap();
    let out = std::env::args().nth(2).unwrap();
    match desqu_desktop::icon_png(&id, 32) { Ok(b) => { std::fs::write(&out, &b).unwrap(); println!("{} bytes", b.len()); } Err(e) => println!("err {e}") }
}
